use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(author, version, about = "Fast local compression with Zstandard", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress a file or directory
    Compress {
        /// Input file or directory
        input: PathBuf,
        /// Output path (defaults to INPUT.zst or INPUT.tar.zst)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Decompress a .zst file or extract a .tar.zst archive
    #[command(alias = "extract")]
    Decompress {
        /// Input archive
        input: PathBuf,
        /// Exact output file or directory (must not already exist)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Place the restored file/directory inside this chosen folder
        #[arg(long, value_name = "DIRECTORY", conflicts_with = "output")]
        into: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Compress { input, output } => compress(&input, output.as_deref()),
        Commands::Decompress {
            input,
            output,
            into,
        } => decompress(&input, output.as_deref(), into.as_deref()),
    }
}

fn compress(input: &Path, output: Option<&Path>) -> Result<()> {
    let input = fs::canonicalize(input)
        .map_err(|error| format!("cannot access input '{}': {error}", input.display()))?;
    let metadata = fs::metadata(&input)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(format!(
            "input '{}' is not a regular file or directory",
            input.display()
        )
        .into());
    }

    let output_path = absolute_output_path(
        output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| default_compressed_path(&input, metadata.is_dir())),
    )?;
    refuse_existing_output(&output_path)?;
    if metadata.is_dir() && output_path.starts_with(&input) {
        return Err("output archive must be outside the input directory".into());
    }

    let threads = std::thread::available_parallelism().map_or(1, |value| value.get());
    println!("Cores detected: {threads}");
    println!("Zstd level: 1 (high-speed mode)");
    println!(
        "Compressing: {} -> {}",
        input.display(),
        output_path.display()
    );
    let start = Instant::now();

    let result = (|| -> Result<()> {
        let output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        let writer = BufWriter::new(output_file);
        let mut encoder = zstd::stream::Encoder::new(writer, 1)?;
        encoder.multithread(0)?;

        if metadata.is_dir() {
            append_directory(&mut encoder, &input)?;
        } else {
            let mut reader = BufReader::new(File::open(&input)?);
            io::copy(&mut reader, &mut encoder)?;
        }
        encoder.finish()?.flush()?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&output_path);
        return Err(error);
    }

    let output_size = fs::metadata(&output_path)?.len();
    println!("Completed in {:.2?}", start.elapsed());
    println!("Output: {} ({output_size} bytes)", output_path.display());
    Ok(())
}

fn append_directory<W: Write>(
    encoder: &mut zstd::stream::Encoder<'_, W>,
    input: &Path,
) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.green} {msg}")?,
    );
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner.set_message("Scanning directory tree...");

    let mut entries = WalkDir::new(input)
        .follow_links(false)
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.path().cmp(right.path()));
    entries.retain(|entry| entry.path() != input);
    spinner.finish_with_message(format!("Found {} items", entries.len()));

    let progress = ProgressBar::new(entries.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )?
            .progress_chars("#>-"),
    );

    let mut archive = tar::Builder::new(encoder);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(input)?;
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(format!("symbolic links are not supported: {}", path.display()).into());
        }
        if file_type.is_dir() {
            archive.append_dir(relative, path)?;
        } else if file_type.is_file() {
            let mut file = File::open(path)?;
            archive.append_file(relative, &mut file)?;
        } else {
            return Err(format!("unsupported filesystem entry: {}", path.display()).into());
        }
        progress.inc(1);
    }
    archive.finish()?;
    progress.finish_with_message("Compression complete");
    Ok(())
}

fn decompress(input: &Path, output: Option<&Path>, into: Option<&Path>) -> Result<()> {
    let input = fs::canonicalize(input)
        .map_err(|error| format!("cannot access archive '{}': {error}", input.display()))?;
    if !fs::metadata(&input)?.is_file() {
        return Err(format!("archive '{}' is not a regular file", input.display()).into());
    }

    let is_tar = input
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".tar.zst"));
    let output_path = decompression_output_path(&input, is_tar, output, into)?;
    refuse_existing_output(&output_path)?;
    println!(
        "Decompressing: {} -> {}",
        input.display(),
        output_path.display()
    );
    let start = Instant::now();

    if is_tar {
        extract_archive_safely(&input, &output_path)?;
    } else {
        let result = (|| -> Result<()> {
            let reader = BufReader::new(File::open(&input)?);
            let mut decoder = zstd::stream::Decoder::new(reader)?;
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)?;
            let mut writer = BufWriter::new(file);
            io::copy(&mut decoder, &mut writer)?;
            writer.flush()?;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&output_path);
            return Err(error);
        }
    }

    println!("Completed in {:.2?}", start.elapsed());
    println!("Output: {}", output_path.display());
    Ok(())
}

fn extract_archive_safely(input: &Path, output: &Path) -> Result<()> {
    let partial = partial_directory(output)?;
    fs::create_dir(&partial)?;

    #[cfg(target_os = "linux")]
    let (sync_tx, sync_rx) = std::sync::mpsc::channel();
    #[cfg(target_os = "linux")]
    let syncer = std::thread::spawn(move || {
        while sync_rx.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = std::process::Command::new("sync").status();
        }
    });

    let result = (|| -> Result<()> {
        let reader = BufReader::new(File::open(input)?);
        let decoder = zstd::stream::Decoder::new(reader)?;
        let mut archive = tar::Archive::new(decoder);
        let mut seen = HashSet::new();

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner:.green} {msg}")?,
        );
        spinner.enable_steady_tick(Duration::from_millis(100));
        spinner.set_message("Extracting files...");
        let mut count = 0;

        for entry in archive.entries()? {
            let mut entry = entry?;
            let relative = entry.path()?.into_owned();
            validate_archive_path(&relative)?;
            if !seen.insert(relative.clone()) {
                return Err(
                    format!("archive contains duplicate path: {}", relative.display()).into(),
                );
            }
            let entry_type = entry.header().entry_type();
            if !entry_type.is_file() && !entry_type.is_dir() {
                return Err(format!(
                    "archive contains unsupported link or entry: {}",
                    relative.display()
                )
                .into());
            }
            if !entry.unpack_in(&partial)? {
                return Err(format!("unsafe archive path rejected: {}", relative.display()).into());
            }
            
            count += 1;
            if count % 100 == 0 {
                spinner.set_message(format!("Extracted {count} items..."));
            }
        }
        spinner.finish_with_message(format!("Extraction complete ({count} items)"));
        Ok(())
    })();

    #[cfg(target_os = "linux")]
    let _ = sync_tx.send(());
    #[cfg(target_os = "linux")]
    let _ = syncer.join();

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&partial);
        return Err(error);
    }
    fs::rename(&partial, output).map_err(|error| {
        let _ = fs::remove_dir_all(&partial);
        format!(
            "cannot finalize output directory '{}': {error}",
            output.display()
        )
        .into()
    })
}

fn validate_archive_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("unsafe archive path: {}", path.display()).into());
    }
    Ok(())
}

fn refuse_existing_output(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(format!("output already exists: {}", path.display()).into());
    }
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn absolute_output_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn default_compressed_path(input: &Path, is_dir: bool) -> PathBuf {
    let mut path = input.to_path_buf();
    let name = input.file_name().unwrap_or_default().to_string_lossy();
    path.set_file_name(if is_dir {
        format!("{name}.tar.zst")
    } else {
        format!("{name}.zst")
    });
    path
}

fn default_decompressed_path(input: &Path, is_tar: bool) -> PathBuf {
    let mut path = input.to_path_buf();
    let name = input.file_name().unwrap_or_default().to_string_lossy();
    let trimmed = if is_tar {
        name.strip_suffix(".tar.zst")
    } else {
        name.strip_suffix(".zst")
    };
    path.set_file_name(
        trimmed
            .filter(|value| !value.is_empty())
            .unwrap_or("output"),
    );
    path
}

fn decompression_output_path(
    input: &Path,
    is_tar: bool,
    output: Option<&Path>,
    into: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(output) = output {
        return absolute_output_path(output.to_path_buf());
    }

    let default = default_decompressed_path(input, is_tar);
    if let Some(directory) = into {
        let directory = absolute_output_path(directory.to_path_buf())?;
        if directory.exists() && !directory.is_dir() {
            return Err(format!(
                "chosen destination is not a directory: {}",
                directory.display()
            )
            .into());
        }
        fs::create_dir_all(&directory)?;
        return Ok(directory.join(default.file_name().unwrap_or_default()));
    }
    absolute_output_path(default)
}

fn partial_directory(output: &Path) -> Result<PathBuf> {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = parent.join(format!(
        ".{name}.fcz-partial-{}-{nonce}",
        std::process::id()
    ));
    if path.exists() {
        return Err("temporary extraction path already exists".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn test_dir(label: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("fcz-{label}-{}-{id}", std::process::id()))
    }

    fn assert_trees_equal(left: &Path, right: &Path) {
        let collect = |root: &Path| {
            let mut paths: Vec<_> = WalkDir::new(root)
                .into_iter()
                .map(|entry| entry.unwrap())
                .filter(|entry| entry.path() != root)
                .map(|entry| entry.path().strip_prefix(root).unwrap().to_path_buf())
                .collect();
            paths.sort();
            paths
        };
        let left_paths = collect(left);
        let right_paths = collect(right);
        assert_eq!(left_paths, right_paths);
        for path in left_paths {
            let left_path = left.join(&path);
            let right_path = right.join(&path);
            if left_path.is_file() {
                assert_eq!(fs::read(left_path).unwrap(), fs::read(right_path).unwrap());
            } else {
                assert!(right_path.is_dir());
            }
        }
    }

    #[test]
    fn directory_round_trip_preserves_names_and_bytes() {
        let root = test_dir("round-trip");
        let source = root.join("source with spaces");
        fs::create_dir_all(source.join("nested/empty-dir")).unwrap();
        fs::write(source.join("empty.txt"), []).unwrap();
        fs::write(source.join("nested/hello.txt"), b"hello\0world").unwrap();
        fs::write(
            source.join("nested/こんにちは.txt"),
            "Unicode contents".as_bytes(),
        )
        .unwrap();
        fs::write(source.join("large-ish.bin"), vec![0xA5; 2 * 1024 * 1024]).unwrap();
        let archive = root.join("archive.tar.zst");
        let restored = root.join("restored output");

        compress(&source, Some(&archive)).unwrap();
        decompress(&archive, Some(&restored), None).unwrap();
        assert_trees_equal(&source, &restored);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_round_trip_preserves_bytes() {
        let root = test_dir("file");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("data file.bin");
        let archive = root.join("data.zst");
        let restored = root.join("restored.bin");
        fs::write(
            &source,
            (0..=255).cycle().take(1024 * 1024).collect::<Vec<_>>(),
        )
        .unwrap();
        compress(&source, Some(&archive)).unwrap();
        decompress(&archive, Some(&restored), None).unwrap();
        assert_eq!(fs::read(source).unwrap(), fs::read(restored).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_output() {
        let root = test_dir("overwrite");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("input.txt");
        let output = root.join("existing.zst");
        fs::write(&source, b"input").unwrap();
        fs::write(&output, b"keep me").unwrap();
        assert!(
            compress(&source, Some(&output))
                .unwrap_err()
                .to_string()
                .contains("already exists")
        );
        assert_eq!(fs::read(output).unwrap(), b"keep me");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsafe_archive_paths() {
        for path in [
            Path::new("../outside"),
            Path::new("/absolute"),
            Path::new("a/../../outside"),
        ] {
            assert!(validate_archive_path(path).is_err());
        }
        #[cfg(windows)]
        assert!(validate_archive_path(Path::new("C:\\outside")).is_err());
        assert!(validate_archive_path(Path::new("nested/file.txt")).is_ok());
    }

    #[test]
    fn rejects_archive_links() {
        let root = test_dir("link");
        fs::create_dir_all(&root).unwrap();
        let archive_path = root.join("link.tar.zst");
        let writer = File::create(&archive_path).unwrap();
        let encoder = zstd::stream::Encoder::new(writer, 1).unwrap();
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_cksum();
        archive
            .append_link(&mut header, "link", "../outside")
            .unwrap();
        let encoder = archive.into_inner().unwrap();
        encoder.finish().unwrap();
        let output = root.join("output");
        assert!(decompress(&archive_path, Some(&output), None).is_err());
        assert!(!output.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extracts_inside_an_existing_chosen_folder() {
        let root = test_dir("chosen-folder");
        let source = root.join("source");
        let chosen = root.join("any folder with spaces");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&chosen).unwrap();
        fs::write(chosen.join("keep-existing.txt"), b"untouched").unwrap();
        fs::write(source.join("hello.txt"), b"hello").unwrap();
        let archive = root.join("my-backup.tar.zst");

        compress(&source, Some(&archive)).unwrap();
        decompress(&archive, None, Some(&chosen)).unwrap();

        assert_eq!(
            fs::read(chosen.join("my-backup/hello.txt")).unwrap(),
            b"hello"
        );
        assert_eq!(
            fs::read(chosen.join("keep-existing.txt")).unwrap(),
            b"untouched"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
