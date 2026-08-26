# midetor

## Description & Demo

`midetor` is a highly customizable, terminal-based Vim-like Markdown editor designed to bring an Obsidian-like experience directly to your command line. Built with Ratatui and Crossterm, it features lightning-fast SQLite-backed metadata management.

`markdown-scanner` is now compiled and integrated directly as an internal library—no external binaries needed! Powered by a deeply integrated Lua scripting engine, `midetor` allows you to completely mold the editor to your workflow, from custom table views to virtual text overlays.

**🚀 Try It Out**

* **Web Emulator:** Experience the editor directly in your browser without installing anything using our **i686 WASM emulator**: [this website](https://midetor.andr.fyi/)

---

## Core Features

* **Integrated Knowledge Graph:** The built-in scanner automatically parses your notes, tags (`#`), YAML frontmatter, and backlinks (`[[`) into a local SQLite database.
* **Rich Media in the Terminal:** Render local images inline (handling aspect ratios natively) and play linked `.mp3` audio files via VLC.
* **Clipboard Image Pasting:** Paste images directly from your system clipboard. `midetor` automatically saves the image to your vault and inserts the Markdown link.
* **Interactive Table Browser:** Query your vault's metadata using SQL via Lua, render interactive tables, and dynamically filter rows in real-time using include/exclude syntax.
* **Advanced Vim Editing:** Full Normal, Insert, Visual, Visual Block, and Command modes, plus a dedicated "Read Mode" for dynamic word-wrapping.
* **Limitless Extensibility:** Use Lua to program custom fuzzy finders, dynamic autocomplete snippets (`@` trigger), virtual "ghost" text, and custom table views without recompiling Rust.

---

## Usage & CLI

Run the editor with the following command:

```bash
midetor <file_path> [base_dir] [music_folder]

```

* `<file_path>`: Path to the Markdown file to edit.
* `[base_dir]`: Base directory of the vault. Defaults to the `OBSIDIAN_VAULT_PATH` environment variable.
* `[music_folder]`: Directory for `.mp3` files. Defaults to the `MUSIC_FOLDER` environment variable.

**Standalone Scanner Subcommand**
Because `markdown-scanner` is built-in, you can manually trigger database scans or cleanups without opening the UI:

```bash
midetor scan <file> <base_dir> [OPTIONS]

```

*(Supports `--json-only`, `--delete`, and `--clean` for orphaned files).*

---

## Lua API & Extensibility

`midetor` is configured entirely via an `init.lua` file located in `~/.config/midetor/`. The editor exposes a global `editor` object with an extensive API for manipulating text, UI, and data.

### Basic Keymapping & Editor Control

You can map keys for any mode (`n`, `i`, `v`, `t`) and trigger complex logic, including clipboard and UI management.

```lua
-- Save and quit
editor:map("n", "<C-s>", function() editor:save() end)
editor:map("n", "<C-q>", function() editor:quit() end)

-- Paste image directly from OS clipboard to a "Files" folder
editor:map("n", "\\p", function() 
    editor:paste_image_from_clipboard("Files") 
end)

-- Toggle word-wrapping Read Mode
editor:map("n", "\\w", function() editor:toggle_read_mode() end)

-- Jump to end of line, insert newline, and enter insert mode
editor:map("n", "o", function()
    editor:move("end")        
    editor:insert_text("\n")  
    editor:set_mode("insert") 
end)

-- Visual block multi-line movement (Alt+Up)
editor:map("v", "<A-Up>", function() 
    local c_row, c_col = editor:get_cursor()
    local a_row, a_col = editor:get_visual_anchor()
    local start_row = math.min(c_row, a_row)
    local lines = editor:get_all_lines()
    
    if start_row > 0 then
        local line_above = table.remove(lines, start_row)
        table.insert(lines, math.max(c_row, a_row) + 1, line_above)
        editor:set_lines(lines)
        editor:set_selection(a_row - 1, a_col)
        editor:set_cursor(c_row - 1, c_col)
    end
end)

```

### Virtual Text Injection

You can inject non-selectable "ghost text" directly into the editor buffer (useful for inline diagnostics or git-blame style info):

```lua
-- Inject inline ghost text at line 0, column 10
editor:set_virtual_text(0, 10, "Author: John Doe", "gray")

```

### Dynamic Templates & Search Providers

You can build custom UI pickers directly in Lua. For example, a dynamic Markdown template picker:

```lua
_G.template_search_provider = function(query)
    local results = {}
    local handle = io.popen('ls -1 "Templates/" 2>/dev/null')
    if handle then
        for file in handle:lines() do
            if file:match("%.md$") and (query == "" or string.find(string.lower(file), string.lower(query), 1, true)) then
                table.insert(results, file)
            end
        end
        handle:close()
    end
    return results
end

_G.template_search_action = function(selected_item)
    if selected_item and selected_item ~= "" then
        editor:process_template("Templates/" .. selected_item)
        editor:set_status("Applied template: " .. selected_item)
    end
end

editor:map("n", "\\nt", function()
    editor:start_custom_search("template_search_provider", "template_search_action")
end)

```

### Database Querying & Custom Tables

Because `markdown-scanner` is built-in, you can execute SQL queries directly against your vault's metadata (including YAML filters) and render them in native terminal tables.

```lua
editor:map("n", "<C-b>", function()
    local query = [[
        SELECT f.file_name as file,
               GROUP_CONCAT(DISTINCT t.tag) as tags,
               strftime('%Y-%m-%d', f.created_at, 'unixepoch') as Date,
               json_extract(f.metadata, '$.Finished') as Done
        FROM files f
        LEFT JOIN file_tags ft ON f.id = ft.file_id
        LEFT JOIN tags t ON ft.tag_id = t.id
        GROUP BY f.id
    ]]

    local cols, raw_rows = editor:query_db(query)
    local formatted_data = {}
    
    for _, row in ipairs(raw_rows) do
        local done_icon = (row[4] == "1" or row[4] == "true") and "✅" or "❌"
        table.insert(formatted_data, { row[1] or "", row[2] or "", row[3] or "", done_icon })
    end

    editor:open_table({
        columns = { "File", "Tags", "Date", "Done" },
        data = formatted_data,
        sort_col = 3,
        sort_asc = false
    })
end)

```

### Autocomplete Snippets (`@`)

Trigger dynamic text expansion in Insert mode by typing `@`:

```lua
local snippets = {
    ["date"] = function() return os.date("%Y-%m-%d") end,
    ["file-name"] = function() 
        return editor:get_current_file():match("^.+/(.+)$") 
    end
}

function on_autocomplete(trigger, query)
    local results = {}
    if trigger == "@" then
        for key, _ in pairs(snippets) do
            if string.sub(key, 1, string.len(query)) == query then table.insert(results, key) end
        end
    end
    return results
end

function expand_autocomplete(trigger, suggestion)
    if trigger == "@" then
        local action = snippets[suggestion]
        if type(action) == "function" then return action() end
        return action
    end
    return suggestion 
end

```

## Internal SQLite Database

The editor automatically maintains a SQLite database (`markdown_data.db`) in your `base_dir` to store metadata. The built-in scanner runs in the background and populates:

* `files`: File paths, names, and YAML metadata JSON.
* `tags`: Unique tags.
* `file_tags`: Mapping files to tags.
* `backlinks`: Tracks `[[wikilinks]]` between files.

## Environment Variables

* `OBSIDIAN_VAULT_PATH`: Default base directory for the vault.
* `MUSIC_FOLDER`: Default directory for audio/music playback.

## License

This project is licensed under the GNU GENERAL PUBLIC LICENSE. See the `LICENSE` file for details.
