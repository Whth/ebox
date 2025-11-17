# Ebox

A comprehensive collection of command-line utilities written in Rust, providing tools for file management, data processing, media handling, and more.

## Overview

Ebox is a suite of over 40 specialized command-line tools designed to handle various tasks efficiently. Each tool is built with performance and usability in mind, leveraging Rust's speed and safety guarantees.

## Installation

### Prerequisites

- Rust toolchain (rustc, cargo)
- FFmpeg (for video/audio processing tools)
- Various system dependencies depending on the tools used

### Building from Source

```bash
git clone <repository-url>
cd ebox
cargo build --release
```

The compiled binaries will be available in `target/release/`.

### Installing Specific Tools

After building, you can install individual tools globally:

```bash
cargo install --path . --bin <tool-name>
```

## Available Tools

### File Management

- **`mf`** - Merge folders with similar names using fuzzy matching
  - Moves files from source folders to matching destination folders
  - Supports Levenshtein distance similarity threshold
  - Interactive folder selection with confidence scoring

- **`sz`** - Get the size of subfolders
  - Displays folder sizes in a sorted table
  - Option to open the largest directory in file explorer

- **`pmov`** - Move directories based on file count thresholds
  - Groups directories into buckets by file count
  - Automatically creates numbered target directories

- **`reduce`** - Reduce directory contents with collision handling strategies
  - Merge or move directory contents with conflict resolution
  - Supports halt, override, and auto strategies

- **`renm`** - Rename files and restore from backups
  - Batch rename with mapping file generation
  - Restore functionality with conflict detection

- **`rstrip`** - Remove lines from CSV files based on delimiters
  - Strip content before/after delimiters in CSV files

- **`rpl`** - Replace file extensions
  - Bulk file extension conversion

- **`wprune`** - Prune Windows shortcut files
  - Remove invalid .lnk files

- **`sufk`** - Add suffixes to files
  - Batch suffix addition with pattern matching

- **`onize`** - Rename files with numeric prefixes
  - Add zero-padded numbering to files

- **`thernam`** - Rename files for better organization
  - Interactive file selection and renaming

- **`verbu`** - Version bumping utility
  - Manage project versions across different file types
  - Support for Cargo.toml, package.json, and more

### Media Processing

- **`vmd`** - Concatenate short videos using FFmpeg
  - Filter videos by duration threshold
  - Hardware acceleration support (NVENC)

- **`avd`** - Extract data from CSV files with column filtering
  - Column-based data extraction and validation

- **`pdfp`** - Process PDF files
  - Extract images from PDFs
  - Remove small files by size threshold

- **`bpdf`** - Batch PDF processing
  - Chunk PDF files into smaller groups
  - Process chunks in parallel

- **`bdocx`** - Generate Word documents
  - Create DOCX files with custom styling

- **`pls`** - Process image files
  - Classify images by properties (grayscale, transparency, size)
  - Extract/filter images based on criteria

- **`pps`** - Classify images by aspect ratios
  - Move images to categorized folders by dimensions

### Data Processing

- **`ag`** - OpenAI CLI interface
  - Chat with OpenAI models via command line
  - Model listing and pinning functionality

- **`adconv`** - Convert and process CSV data
  - Handle timestamp columns and data transformations

- **`ncccsv`** - Process NetCDF data to CSV
  - Convert NetCDF files to CSV format

- **`yldt`** - Split datasets for machine learning
  - Train/validation/test split with YAML generation

- **`mpr`** - Process Markdown images
  - Update image paths in Markdown files
  - Batch rename referenced images

- **`hive`** - Parse citation patterns in files
  - Extract and count `#cite()` references

- **`corda`** - Citation pattern analysis
  - Process academic citation formats

- **`ciklen`** - Process file lengths/counts

- **`pam`** - Multimedia file processing
  - Audit, merge, and manage media libraries

- **`recmd`** - Execute commands in directories
  - Batch command execution across directory trees

- **`xect`** - Image conversion utilities
  - Convert between image formats (PNG, etc.)

### System Utilities

- **`startups`** - Manage Windows startup programs
  - Add/remove startup entries

- **`cdd`** - Download from CSV URLs
  - Batch download from URL lists in CSV files

- **`cpr`** - Copy files with progress indication
  - Parallel file copying with progress bars

- **`fof`** - Find files by extensions
  - Recursive file discovery by extension

- **`vlc`** - Version control utilities
  - Git repository management and analysis

- **`ffoil`** - File organization utilities
  - Advanced file system operations

- **`makwei`** - Weight/bias file processing

### Game Mod Management

- **`facm`** - Factorio mod manager
  - Install, update, and manage Factorio mods
  - Dependency resolution and version management

## Usage Examples

### Folder Merging with Fuzzy Matching
```bash
mf source_folder destination_folder --threshold 0.8
```

### Video Concatenation
```bash
vmd --dir ./videos --max-duration 30 --output combined.mp4
```

### Directory Size Analysis
```bash
sz --explore-greatest-dir
```

### OpenAI Chat Interface
```bash
export OPENAI_API_KEY=your_key_here
ag chat "Explain quantum computing in simple terms"
```

### Factorio Mod Management
```bash
facm install "mod-name"
```

### Dataset Splitting
```bash
yldt --input-dir ./data --train-ratio 0.7 --val-ratio 0.2 --test-ratio 0.1
```

## Configuration

Some tools support configuration files:

- `ag` (OpenAI CLI): `~/.config/openai-cli/config.toml`

## Dependencies

Ebox uses several external crates and system dependencies:

- **FFmpeg** for media processing
- **OpenAI API** for AI features
- **Various Rust crates** for specific functionalities (see Cargo.toml)

## Contributing

Contributions are welcome! Please ensure:

1. Code follows Rust best practices
2. New tools include proper CLI argument parsing with `clap`
3. Error handling is robust
4. Documentation is updated

## License

This project is licensed under the MIT License, see [LICENSE](LICENSE).

## Version

Current version: 0.1.37