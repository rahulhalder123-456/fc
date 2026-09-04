[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)] [string] $InputDirectory,
    [string] $Output
)

$ErrorActionPreference = "Stop"
$inputPath = (Resolve-Path -LiteralPath $InputDirectory).Path
if (-not (Test-Path -LiteralPath $inputPath -PathType Container)) { throw "Input must be a directory." }
if (-not $Output) { $Output = Join-Path (Split-Path $inputPath -Parent) ((Split-Path $inputPath -Leaf) + ".benchmark.tar.zst") }
$outputPath = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $outputPath) { throw "Output already exists: $outputPath" }

$files = @(Get-ChildItem -LiteralPath $inputPath -File -Recurse -Force)
$folders = @(Get-ChildItem -LiteralPath $inputPath -Directory -Recurse -Force)
$inputBytes = [long](($files | Measure-Object -Property Length -Sum).Sum)
$fcz = (Get-Command fcz -ErrorAction Stop).Source

Write-Host "fcz benchmark"
Write-Host "OS: $([Runtime.InteropServices.RuntimeInformation]::OSDescription)"
Write-Host "Architecture: $([Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
Write-Host "Logical processors: $([Environment]::ProcessorCount)"
Write-Host "Input: $inputPath"
Write-Host "Files: $($files.Count); folders: $($folders.Count); bytes: $inputBytes"

$watch = [Diagnostics.Stopwatch]::StartNew()
& $fcz compress $inputPath --output $outputPath
$exit = $LASTEXITCODE
$watch.Stop()
if ($exit -ne 0) { throw "fcz failed with exit code $exit" }

$outputBytes = (Get-Item -LiteralPath $outputPath).Length
$seconds = $watch.Elapsed.TotalSeconds
$throughput = if ($seconds -gt 0) { $inputBytes / $seconds / 1MB } else { 0 }
$ratio = if ($inputBytes -gt 0) { 100 * $outputBytes / $inputBytes } else { 0 }
Write-Host ("Elapsed: {0:N2} seconds" -f $seconds)
Write-Host "Archive bytes: $outputBytes"
Write-Host ("Approximate throughput: {0:N2} MiB/s" -f $throughput)
Write-Host ("Compression ratio: {0:N2}% (archive/input)" -f $ratio)
Write-Host "Output: $outputPath"
