# midetor

## Description

`midetor` (MY-EDITOR) is a terminal-based vim like Markdown editor designed to provide a lightweight, Obsidian-like experience for editing Markdown files. Works only with [markdown-scanner](https://github.com/andrenaP/markdown-scanner). It supports syntax highlighting, tag management, and backlink tracking, storing metadata in a SQLite database (`markdown_data.db`). The editor uses a TUI (Text User Interface) built with Ratatui and Crossterm, offering an ~~intuitive interface~~ for navigating and editing Markdown files.

## Trying it out
Go to [this repo](https://github.com/andrenaP/midetor-docker-tesiting) and run it inside `Docker`. You can pass `-v` to volume Your folder if You want. 

## Why?
- Do you want nvim that don't break a few times a week? This thing will not break. And the best part **You can just copy it on any device with terminal and it will work out of a box**
- You can use [This website](https://github.com/andrenaP/database-reader-sql) and render your data in userfriendly interface.
- This editor is just an `example of how you can work with markdown-scanner`

![images/main.jpg](https://github.com/andrenaP/midetor/blob/aadcee84d86bc2e4686d600950c919c017e5a820/images/main.jpg)

## Features

- Edit Markdown files with syntax highlighting.
- Manage tags `#`, backlinks `[[` stored in a SQLite database and custom autocomplete options `@`.
- Support for Obsidian-like vault structures. (For now only `[[this type]]`)

## Requirements

- **Rust**: Version 1.87.0 or higher.
- **Cargo**: The Rust package manager.
- A [markdown-scanner](https://github.com/andrenaP/markdown-scanner) binary (assumed to be available in the system PATH) to populate the database.

## Installation

1. **Install midetor**:
   Copy the compiled binary to a directory in your PATH:
   ```bash
   cargo install --git https://github.com/andrenaP/midetor.git
   ```

2. **Install `markdown-scanner`**:
   The editor requires a `markdown-scanner` binary to process Markdown files and populate the database. Ensure it is installed and accessible in your system
   ```bash
   cargo install --git https://github.com/andrenaP/markdown-scanner.git
   ```
## Usage

Run the editor with the following command:

```bash
markdown-editor <file_path> [base_dir]
```

- `<file_path>`: Path to the Markdown file to edit (required).
- `[base_dir]`: Base directory of the Obsidian vault (optional). Defaults to the `Obsidian_valt_main_path` environment variable or the current working directory if not set.

### Examples

- Edit a file using the default vault path:
  ```bash
  markdown-editor notes.md
  ```

- Edit a file with a specific vault directory:
  ```bash
  markdown-editor notes.md /path/to/vault
  ```

- View help:
  ```bash
  markdown-editor --help
  ```

### Key Bindings

Well this is complicated. It works like vim `:wq` `:w` `:q`.

- For input go to Insert mode with `i`.
- For selection go to visual mode with `v`.
- `\ot` for tags.
- `\ob` for backlinks.
- `\f` search
- `\oot` `\ooT` `\ooy` open dayly files.
- `\t` open `FileTreeVisual`. `oc`, `on` to sort my time or name. Other: `y` for copy, `x` for cut `p`, for paste, `v` for selection.
- `\nt` Makes autocomplete from Templates `Look Obsidian Templates if you are interested`.


## Database

The editor uses a SQLite database (`markdown_data.db`) in the `base_dir` to store metadata about files, tags, and backlinks. If the database does not exist, it is automatically created with the following schema:

- `files`: Stores file paths and names.
- `tags`: Stores unique tags.
- `file_tags`: Maps files to tags.
- `backlinks`: Tracks backlinks between files.

The `markdown-scanner` tool is executed to populate the database when a new file is opened.

## Environment Variables

- `Obsidian_valt_main_path`: Specifies the default base directory for the vault if not provided via the command line. You can use it if you need.

## License

This project is licensed under the GNU GENERAL PUBLIC LICENSE License. See the `LICENSE` file for details.

## Contact

For issues or questions, please open an issue on the repository.
