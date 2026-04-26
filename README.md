# midetor

## Description

`midetor` (MY-EDITOR) is a terminal-based Vim-like Markdown editor designed to provide a highly customizable, Obsidian-like experience right in your terminal. It integrates tightly with [markdown-scanner](https://github.com/andrenaP/markdown-scanner) to provide lightning-fast SQLite-backed metadata management. 

Powered by Ratatui and Crossterm, `midetor` now features **embedded Lua scripting**, allowing you to completely customize keybindings, write custom search providers, and build your own autocomplete snippets. 

## Trying it out
Go to [this repo](https://github.com/andrenaP/midetor-docker-tesiting) and run it inside `Docker`. You can pass `-v` to volume your folder if you want.

## Why?
- Do you want an `nvim`-like experience that doesn't break a few times a week? This editor is heavily self-contained. 
- **You can just copy it on any device with a terminal and it will work out of the box.**
- You can use [this website](https://github.com/andrenaP/database-reader-sql) to render your data in a user-friendly interface.
- It is fully **extensible via Lua** without needing to recompile the Rust source.
- This editor serves as an advanced example of how you can work with `markdown-scanner`.

![images/main.jpg](https://github.com/andrenaP/midetor/blob/aadcee84d86bc2e4686d600950c919c017e5a820/images/main.jpg)

It can now render images (using backlinks) directly in the terminal!
![images/images-render-example.png](https://github.com/andrenaP/midetor/blob/2c23333e6a1ea811a73961963ba739051a3099f1/images/images-render-example.png)

## Features

- **Extensible Configuration:** Write an `init.lua` file to define keymaps, macros, and UI behaviors.
- **Media Playback:** Render local images in the terminal and play linked `.mp3` audio files via VLC.
- **Advanced Editing:** Vim-like Normal, Insert, Visual, and Command modes.
- **Virtual Text & UI:** Inject custom text overlays into the editor buffer via Lua.
- **Knowledge Graph Support:** Manage tags (`#`) and backlinks (`[[`) via a fast SQLite database.
- **Custom Search & Autocomplete:** Program your own fuzzy finders and snippet expanders using Lua (`@` trigger).

## Requirements

- **Rust**: Version 1.87.0 or higher.
- **Cargo**: The Rust package manager.
- **markdown-scanner**: Must be available in your system PATH to populate the database.
- **VLC (Optional)**: Required if you want to play audio backlinks from the editor.

## Installation

1. **Install midetor**:
   Copy the compiled binary to a directory in your PATH:
   ```bash
   cargo install --git https://github.com/andrenaP/midetor.git
   ```

2. **Install `markdown-scanner`**:
   The editor requires this binary to process Markdown files and generate the database.
   ```bash
   cargo install --git https://github.com/andrenaP/markdown-scanner.git
   ```

## Usage

Run the editor with the following command:

```bash
midetor <file_path> [base_dir] [music_folder]
```

- `<file_path>`: Path to the Markdown file to edit (required).
- `[base_dir]`: Base directory of the Obsidian vault (optional). Defaults to the `Obsidian_valt_main_path` env variable or the current working directory.
- `[music_folder]`: Directory where `.mp3` files are stored (optional). Defaults to the `musik_folder` env variable or the current working directory.

### Examples

- Edit a file using the default vault path:
  ```bash
  midetor notes.md
  ```

- Edit a file with a specific vault directory and music folder:
  ```bash
  midetor notes.md /path/to/vault /path/to/music
  ```

## Configuration & Lua Scripting

`midetor` reads an `init.lua` file from your current working directory on startup or from `~/,config/midetor/`. This is where you configure all keybindings, custom macros, search logic, and snippet expansions.

The editor exposes a global `editor` object to Lua.

### Mapping Keys
You can map keys for Normal (`n`) and Visual (`v`) modes. Use standard characters or angle brackets for special keys (e.g., `<C-s>`, `<Esc>`, `<A-Up>`).

```lua
-- Save file
editor:map("n", "<C-s>", function() editor:save() end)

-- Toggle File Tree
editor:map("n", "\\t", function() editor:toggle_file_tree() end)

-- Create a custom macro (Jump to end of line, add newline, enter insert mode)
editor:map("n", "o", function()
    editor:move("end")        
    editor:insert_text("\n")  
    editor:set_mode("insert") 
end)
```

### Custom Autocomplete Snippets (`@`)
You can define dynamic snippets in Lua that trigger when you type `@` in Insert mode. You must define two global functions: `on_autocomplete` and `expand_autocomplete`.

```lua
local snippets = {
    ["date"] = function() return os.date("%Y-%m-%d") end,
    ["file-name"] = function() return editor:get_current_file():match("^.+/(.+)$") end
}

function on_autocomplete(trigger, query)
    local results = {}
    if trigger == "@" then
        for key, _ in pairs(snippets) do
            if string.sub(key, 1, string.len(query)) == query then
                table.insert(results, key)
            end
        end
    end
    return results
end

function expand_autocomplete(trigger, suggestion)
    if trigger == "@" then
        local action = snippets[suggestion]
        if type(action) == "function" then return action() end
        if type(action) == "string" then return action end
    end
    return suggestion 
end
```

### Custom Search Providers
You can build custom search interfaces directly in Lua by providing a search function and a selection callback.

```lua
_G.my_search_provider = function(query)
    -- Return a table of strings based on the query
    return {"apple", "banana", "cherry"}
end

_G.my_search_action = function(selected_item)
    editor:insert_text("Selected: " .. selected_item)
end

-- Bind it to a key
editor:map("n", "\\fs", function()
    editor:start_custom_search("my_search_provider", "my_search_action")
end)
```

## Default Key Bindings (Provided via `init.lua`)

If you use the example `init.lua`, the following default bindings are active:

| Keybinding | Mode | Action |
| :--- | :--- | :--- |
| `i` / `a` | Normal | Enter Insert mode |
| `v` / `<C-v>` | Normal | Enter Visual / Visual Block mode |
| `:` | Normal | Enter Command mode (`:w`, `:q`, `:wq`) |
| `<C-s>` / `<C-q>`| Normal | Save file / Quit |
| `u` / `<C-r>` | Normal | Undo / Redo |
| `\t` | Normal | Toggle File Tree |
| `\ob` / `\ot` | Normal | Search Backlinks / Search Tags |
| `\f` | Normal | Search Files |
| `\os` | Normal | Search via Custom SQL query |
| `\if` / `\ic` | Normal | Toggle Image Fullscreen / Clear Image |
| `\s` | Normal | Stop audio playback |
| `\w` | Normal | Toggle Read Mode (Line wrapping) |
| `\nt` | Normal | Open Template picker |
| `\oot` | Normal | Open Today's Daily Note |

*Note: The File Tree has its own bindings once opened (`oc` to sort by time, `on` to sort by name, `y` to copy, `x` to cut, `p` to paste, `d` to delete).*

## Database

The editor creates a SQLite database (`markdown_data.db`) in the `base_dir` to store metadata. The schema includes:
- `files`: File paths and names.
- `tags`: Unique tags.
- `file_tags`: Mapping files to tags.
- `backlinks`: Tracks `[[backlinks]]` between files.

The `markdown-scanner` tool runs automatically in the background to keep this database populated.

## Environment Variables

- `OBSIDIAN_VAULT_PATH`: Default base directory for the vault.
- `MUSIC_FOLDER`: Default directory for audio/music playback.

## License

This project is licensed under the GNU GENERAL PUBLIC LICENSE. See the `LICENSE` file for details.

## Contact

For issues or questions, please open an issue on the repository.
