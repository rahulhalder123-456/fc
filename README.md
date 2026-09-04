# fcz

`fcz` is a cross-platform command-line tool for compressing files and directories with Zstandard. Directory archives use the standard tar format inside a `.tar.zst` stream.

`fcz` is free and open source. It runs locally and does not require an account, API key, subscription, telemetry service, or cloud service.

## Why fcz

- One command for files or complete directory trees
- Zstandard level 1 and multithreaded compression for high-speed local archiving
- Bounded memory use: file contents are streamed into the archive
- Clear output paths and errors
- Existing outputs are never overwritten silently
- Safe directory extraction rejects traversal paths, links, and duplicate entries

## Benchmark

One measured Windows test produced the following result. It represents one machine and one workload, not universal performance.

| Item | Measured value |
| --- | ---: |
| Input directory | 14.0 GB |
| Files | 28,752 |
| Folders | 4,497 |
| Total items reported | 33,250 |
| Logical CPU cores detected by `fcz` | 16 |
| Compression mode | Zstd level 1 |
| Elapsed time | 48.63 seconds |
| Output archive | 1,828,956,895 bytes (about 1.70 GiB) |

Command used:

```powershell
fcz compress "C:\Users\Rahul Halder\Desktop\ty-backend"
```

Output: `ty-backend.tar.zst`

Using the reported 14.0 GB input size, this is approximately **288 MB/s**, a **13.1% archive-to-input ratio**, or an **86.9% size reduction**. These derived figures are approximate because the input size was rounded.

### Methodology and limitations

Record the `fcz` version, exact command, input byte count, item counts, archive byte count, and wall-clock duration. Results vary with CPU, SSD/storage speed, file count, file sizes, compressibility, filesystem, operating system, and compression level. No ZIP or 7-Zip comparison is claimed without running those tools against the same input under the same conditions.

## Installation

Prebuilt installers fetch the latest compatible asset from [GitHub Releases](https://github.com/rahulhalder123-456/fc/releases). They install per-user and do not require Rust or administrator privileges. A platform installer reports a clear error when its release asset has not yet been published.

### Windows x86_64

```powershell
irm https://raw.githubusercontent.com/rahulhalder123-456/fc/master/install.ps1 | iex
```

Installs `fcz.exe` to `%USERPROFILE%\.local\bin` and adds that directory to the user PATH when needed.

### Linux / WSL x86_64

```sh
curl -fsSL https://raw.githubusercontent.com/rahulhalder123-456/fc/master/install.sh | sh
```

### macOS Intel or Apple Silicon

```sh
curl -fsSL https://raw.githubusercontent.com/rahulhalder123-456/fc/master/install.sh | sh
```

The Unix installer uses `~/.local/bin`. If it is not already in PATH, the installer prints the exact command to add it.

### Build from source

Install a stable Rust toolchain, then run:

```sh
cargo install --git https://github.com/rahulhalder123-456/fc.git
```

## Usage

```text
fcz compress <INPUT> [--output <OUTPUT>]
fcz decompress <INPUT> [--output <OUTPUT>]
```

Examples:

```sh
fcz compress "/path/with spaces/project"
fcz compress data.bin --output data.bin.zst
fcz decompress project.tar.zst --output restored-project
fcz decompress data.bin.zst --output restored-data.bin
```

A directory named `project` defaults to `project.tar.zst`. Extracting that archive defaults to a new `project` directory. Choose another `--output` when the default already exists.

## Benchmarking yourself

The repository includes equivalent benchmark helpers. They preserve the input and refuse to overwrite an existing benchmark archive.

```powershell
.\scripts\benchmark.ps1 -InputDirectory "C:\path\to\folder"
```

```sh
./scripts/benchmark.sh /path/to/folder
```

Each script reports input size, file/folder counts, elapsed time, archive size, approximate throughput, compression ratio, system information, and the output path.

## Architecture

For a directory, `fcz` walks and sorts entries for stable ordering, writes them to a tar stream, and sends that stream through the Zstandard encoder. Regular files are streamed instead of being loaded wholesale into memory. Zstandard uses the machine's available worker threads. A plain file skips tar and is written directly as a `.zst` stream.

## Security

Extraction occurs in a temporary sibling directory and is renamed into place only after success. Archive paths containing parent traversal, absolute roots, or Windows prefixes are rejected. Symbolic links, hard links, special entries, and duplicate paths are also rejected so an archive cannot redirect later writes outside the destination. Existing output files and directories are preserved.

Please report vulnerabilities according to [SECURITY.md](SECURITY.md).

## Supported platforms

| Platform | Release asset |
| --- | --- |
| Windows x86_64 | `fcz-windows-x86_64.exe` |
| Linux / WSL x86_64 | `fcz-linux-x86_64` |
| macOS Intel | `fcz-macos-x86_64` |
| macOS Apple Silicon | `fcz-macos-aarch64` |

Only assets actually built for their named target are published. The release workflow can build all four when GitHub Actions is available; local manual release scripts are under `scripts/`.

## Troubleshooting

- **Output already exists:** use `--output` with a new path or move the existing output.
- **Command not found after installation:** open a new terminal, or add `~/.local/bin` to PATH as printed by the installer.
- **Release asset missing:** use the source installation fallback or wait for that platform asset to be published.
- **Symbolic link rejected during compression:** archive the resolved target explicitly; links are excluded to keep cross-platform extraction predictable.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Changes should pass formatting, checks, tests, Clippy, and a release build.

## Uninstallation

```sh
rm ~/.local/bin/fcz
```

```powershell
Remove-Item "$HOME\.local\bin\fcz.exe"
```

## License

Licensed under the [MIT License](LICENSE).
