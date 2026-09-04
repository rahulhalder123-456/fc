use clap::{Parser, Subcommand};
use crossbeam_channel::bounded;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress a file or directory
    #[command(alias = "compose")]
    Compress {
        /// Path to the input file or directory
        input: PathBuf,
        /// Path to the output compressed file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Decompress a file
    Decompress {
        /// Path to the input compressed file
        input: PathBuf,
        /// Path to the output decompressed file or directory
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

enum EntryMsg {
    Dir {
        full_path: PathBuf,
        rel_path: PathBuf,
    },
    FileBuf {
        rel_path: PathBuf,
        metadata: std::fs::Metadata,
        data: Vec<u8>,
    },
    FileStream {
        full_path: PathBuf,
        rel_path: PathBuf,
    },
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress { input, output } => {
            let input = std::fs::canonicalize(&input).unwrap_or(input.clone());
            let metadata = std::fs::metadata(&input)?;
            let is_dir = metadata.is_dir();

            let output_path = output.unwrap_or_else(|| {
                let mut path = input.clone();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let new_file_name = if is_dir {
                    format!("{}.tar.zst", file_name)
                } else {
                    format!("{}.zst", file_name)
                };
                path.set_file_name(new_file_name);
                path
            });

            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            let level = 1;

            println!("Auto-detecting optimal settings...");
            println!("- Cores detected: {} -> Using maximum parallel threading", threads);
            println!("- Zstd Level: {} -> Optimized for extremely fast compression", level);

            let type_str = if is_dir { "directory" } else { "file" };
            println!("Compressing {}: {} -> {}", type_str, input.display(), output_path.display());
            
            let start = Instant::now();

            let output_file = File::create(&output_path)?;
            let writer = BufWriter::new(output_file);
            let mut encoder = zstd::stream::Encoder::new(writer, level)?;
            encoder.multithread(0)?;
            encoder.long_distance_matching(true)?;

            if is_dir {
                let base_path = input.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
                
                let spinner = ProgressBar::new_spinner();
                spinner.set_style(ProgressStyle::default_spinner()
                    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                    .template("{spinner:.green} {msg}")
                    .unwrap());
                spinner.enable_steady_tick(Duration::from_millis(100));
                spinner.set_message("Scanning directory tree...");

                let mut entries: Vec<_> = WalkDir::new(&input)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .collect();
                
                // Sort entries alphabetically (found to be best for node_modules locality)
                entries.sort_by(|a, b| a.path().cmp(b.path()));
                
                spinner.finish_with_message(format!("Found {} items.", entries.len()));

                let pb = ProgressBar::new(entries.len() as u64);
                pb.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"));

                // (tx, rx) bounded queue
                let (tx, rx) = bounded::<EntryMsg>(100);
                let pb_clone = pb.clone();

                let writer_thread = std::thread::spawn(move || -> io::Result<()> {
                    let mut builder = tar::Builder::new(encoder);
                    
                    for msg in rx {
                        match msg {
                            EntryMsg::Dir { full_path, rel_path } => {
                                builder.append_dir(&rel_path, &full_path)?;
                            }
                            EntryMsg::FileBuf { rel_path, metadata, data } => {
                                let mut header = tar::Header::new_gnu();
                                #[allow(unused_must_use)]
                                {
                                    header.set_metadata_in_mode(&metadata, tar::HeaderMode::Complete);
                                }
                                header.set_size(data.len() as u64);
                                header.set_cksum();
                                builder.append_data(&mut header, &rel_path, data.as_slice())?;
                            }
                            EntryMsg::FileStream { full_path, rel_path } => {
                                let mut file = File::open(&full_path)?;
                                builder.append_file(&rel_path, &mut file)?;
                            }
                        }
                        pb_clone.inc(1);
                    }
                    
                    let encoder = builder.into_inner()?;
                    encoder.finish()?;
                    Ok(())
                });

                // Chunked processing: Process 500 files at a time to preserve absolute alphabetical order
                for chunk in entries.chunks(500) {
                    // Read the 500 files into memory in parallel
                    let processed_chunk: Vec<Option<EntryMsg>> = chunk.par_iter().map(|entry| {
                        let full_path = entry.path().to_path_buf();
                        let rel_path = full_path.strip_prefix(&base_path).unwrap_or(&full_path).to_path_buf();
                        
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_dir() {
                                Some(EntryMsg::Dir { full_path, rel_path })
                            } else if metadata.is_file() {
                                let size = metadata.len();
                                if size < 50 * 1024 * 1024 {
                                    if let Ok(data) = std::fs::read(&full_path) {
                                        Some(EntryMsg::FileBuf { rel_path, metadata, data })
                                    } else {
                                        Some(EntryMsg::FileStream { full_path, rel_path })
                                    }
                                } else {
                                    Some(EntryMsg::FileStream { full_path, rel_path })
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    }).collect();

                    // Send the processed files sequentially into the tar channel to guarantee order
                    for msg in processed_chunk.into_iter().flatten() {
                        tx.send(msg).unwrap();
                    }
                }

                drop(tx);
                writer_thread.join().expect("Writer thread panicked")?;
                pb.finish_with_message("Compression complete");

                let duration = start.elapsed();
                let output_metadata = std::fs::metadata(&output_path)?;
                println!(
                    "Successfully compressed directory in {:.2?}. Output Size: {} bytes",
                    duration,
                    output_metadata.len()
                );
            } else {
                let input_file = File::open(&input)?;
                let mut reader = BufReader::new(input_file);
                io::copy(&mut reader, &mut encoder)?;
                encoder.finish()?;
                
                let duration = start.elapsed();
                let output_metadata = std::fs::metadata(&output_path)?;
                let ratio = if metadata.len() > 0 {
                    output_metadata.len() as f64 / metadata.len() as f64 * 100.0
                } else {
                    0.0
                };
                println!(
                    "Successfully compressed file in {:.2?}. Size: {} bytes -> {} bytes ({:.1}% of original size)",
                    duration,
                    metadata.len(),
                    output_metadata.len(),
                    ratio
                );
            }
        }
        Commands::Decompress { input, output } => {
            let is_tar = input.file_name().unwrap_or_default().to_string_lossy().ends_with(".tar.zst");

            let output_path = output.clone().unwrap_or_else(|| {
                let mut path = input.clone();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if is_tar {
                    if file_name.len() > 8 {
                        path.set_file_name(&file_name[..file_name.len() - 8]);
                    } else {
                        path.set_file_name(format!("{}.dec", file_name));
                    }
                } else if file_name.ends_with(".zst") {
                    path.set_file_name(&file_name[..file_name.len() - 4]);
                } else {
                    path.set_file_name(format!("{}.dec", file_name));
                }
                path
            });

            println!("Decompressing: {} -> {}", input.display(), output_path.display());
            let start = Instant::now();

            let input_file = File::open(&input)?;
            let reader = BufReader::new(input_file);
            let mut decoder = zstd::stream::Decoder::new(reader)?;

            if is_tar {
                let extract_dir = if output.is_some() {
                    output_path.clone()
                } else {
                    input.parent().unwrap_or(std::path::Path::new("")).to_path_buf()
                };
                let mut archive = tar::Archive::new(decoder);
                archive.unpack(&extract_dir)?;
            } else {
                let output_file = File::create(&output_path)?;
                let mut writer = BufWriter::new(output_file);
                io::copy(&mut decoder, &mut writer)?;
                drop(writer);
            }

            let duration = start.elapsed();
            println!("Successfully decompressed in {:.2?}.", duration);
        }
    }

    Ok(())
}
