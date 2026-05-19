-- init.lua
-- Normal Mode Mappings

editor:map("n", "gg", function() editor:move_to_top() end)
editor:map("n", "yy", function() editor:yank_line() end)
editor:map("n", "dd", function() editor:delete_line() end)

-- Searching
editor:map("n", "\\ob", function() editor:start_search("backlinks") end)
editor:map("n", "\\ot", function() editor:start_search("tags") end)
editor:map("n", "\\f", function() editor:start_search("files") end)
editor:map("n", "\\os", function() editor:start_search("sql") end)

-- File Tree
editor:map("n", "\\t", function() editor:toggle_file_tree() end)

-- UI / Modes
editor:map("n", "\\if", function() editor:toggle_image_fullscreen() end)
editor:map("n", "\\s", function() editor:stop_audio() end)
editor:map("n", "\\w", function() editor:toggle_read_mode() end)

-- Templates
editor:map("n", "\\nt", function() editor:process_template("Templates/Yaml-Template.md") end)
editor:map("n", "\\nm", function() editor:process_template("Templates/meeting.md") end)
editor:map("n", "\\ig", function() editor:process_template("Templates/img-gallery.md") end)

-- Dynamic Date Files using Lua's os.date!
-- %Y-%m-%d formats to 2026-04-23
editor:map("n", "\\oot", function()
    local today = os.date("Every day info/%Y-%m-%d.md")
    ---@cast today string
    editor:open_file(today)
end)

editor:map("n", "\\ooy", function()
    -- 86400 seconds = 1 day
    local yesterday = os.date("Every day info/%Y-%m-%d.md", os.time() - 86400)
    ---@cast yesterday string
    editor:open_file(yesterday)
end)

editor:map("n", "\\ooT", function()
    local tomorrow = os.date("Every day info/%Y-%m-%d.md", os.time() + 86400)
    ---@cast tomorrow string
    editor:open_file(tomorrow)
end)

-- Single key movement mappings
editor:map("n", "k", function() editor:move("up") end)
editor:map("n", "j", function() editor:move("down") end)
editor:map("n", "h", function() editor:move("left") end)
editor:map("n", "l", function() editor:move("right") end)
editor:map("n", "<Up>", function() editor:move("up") end)
editor:map("n", "<Down>", function() editor:move("down") end)

-- Jump movement
editor:map("n", "gg", function() editor:move("top") end)
editor:map("n", "G", function() editor:move("bottom") end)
editor:map("n", "^", function() editor:move("head") end)
editor:map("n", "$", function() editor:move("end") end)

-- Word movement
editor:map("n", "w", function() editor:move("word_forward") end)
editor:map("n", "b", function() editor:move("word_back") end)
editor:map("n", "<C-Right>", function() editor:move("word_forward") end)
editor:map("n", "<C-Left>", function() editor:move("word_back") end)

-- State changes
editor:map("n", "i", function() editor:set_mode("insert") end)
editor:map("n", "a", function()
    editor:move("right")
    editor:set_mode("insert")
end)
editor:map("n", "o", function() editor:insert_line_below() end)
editor:map("n", "v", function() editor:set_mode("visual") end)
editor:map("n", "<C-v>", function() editor:set_mode("visual_block") end)
editor:map("n", ":", function() editor:set_mode("command") end)

-- Actions
editor:map("n", "u", function() editor:undo() end)
editor:map("n", "<C-r>", function() editor:redo() end)
editor:map("n", "yy", function() editor:yank_line() end)
editor:map("n", "dd", function() editor:delete_line() end)
editor:map("n", "p", function() editor:paste() end)

-- Editor control
editor:map("n", "<C-s>", function() editor:save() end)
editor:map("n", "<C-q>", function() editor:quit() end)


-- Add these to your init.lua file:

-- The arrow keys should now work with the fixed event_to_string!
editor:map("n", "<Left>", function() editor:move("left") end)
editor:map("n", "<Right>", function() editor:move("right") end)
editor:map("n", "<Up>", function() editor:move("up") end)
editor:map("n", "<Down>", function() editor:move("down") end)

-- Handle Enter (Links/Audio) and Escape (Fullscreen closing)
editor:map("n", "<Enter>", function() editor:follow_link() end)

-- History Navigation (Ctrl+o, Ctrl+i)
editor:map("n", "<C-o>", function() editor:navigate_back() end)
editor:map("n", "<C-i>", function() editor:navigate_forward() end)

-- Home and End Keys
editor:map("n", "<Home>", function() editor:move("head") end)
editor:map("n", "<End>", function() editor:move("end") end)

-- Ctrl + Home / End
editor:map("n", "<C-Home>", function() editor:move("top") end)
editor:map("n", "<C-End>", function() editor:move("bottom") end)

-- Ctrl + Up / Down
editor:map("n", "<C-Up>", function() editor:move("up") end)
editor:map("n", "<C-Down>", function() editor:move("down") end)

-- Escape Mapping
editor:map("n", "<Esc>", function()
    editor:cancel()
    editor:set_status("Normal Mode")
end)


function explore()
    local path = os.getenv("Obsidian_valt_main_path")
    if not path then return editor:echo("Error: Env var not set") end

    local current_file_path = editor:get_current_file()
    local script_path = string.format('markdown-scanner "%s" "%s" --json-only', current_file_path, path)

    local handle = assert(io.popen(script_path))
    local result = handle:read("*a")
    handle:close()

    editor:echo(result .. " THIS IS RUST")
end

editor:map("n", "<C-y>", function() explore() end)


editor:map("n", "o", function()
    editor:move("end")        -- Jump to the end of the current line
    editor:insert_text("\n")  -- Insert a newline character
    editor:set_mode("insert") -- Switch to insert mode
end)
-- Implement 'O' (Insert line above) entirely in Lua
editor:map("n", "O", function()
    local row, col = editor:get_cursor()
    local lines = editor:get_all_lines()

    -- Insert at the current line (pushes current line down)
    table.insert(lines, row + 1, "")

    editor:set_lines(lines)
    editor:set_cursor(row, 0) -- Cursor stays on the same Rust row index
    editor:set_mode("insert")
end)


-- ==========================================
-- 1. NORMAL MODE: Single Line Movement
-- ==========================================
local function move_line_up()
    local row, col = editor:get_cursor()
    if row > 0 then
        local lines = editor:get_all_lines()
        -- Swap current line (row + 1) with line above (row)
        lines[row + 1], lines[row] = lines[row], lines[row + 1]
        editor:set_lines(lines)
        editor:set_cursor(row - 1, col)
    end
end

local function move_line_down()
    local row, col = editor:get_cursor()
    local lines = editor:get_all_lines()
    if row < #lines - 1 then
        -- Swap current line (row + 1) with line below (row + 2)
        lines[row + 1], lines[row + 2] = lines[row + 2], lines[row + 1]
        editor:set_lines(lines)
        editor:set_cursor(row + 1, col)
    end
end

-- Map to "n" (Normal mode)
editor:map("n", "<A-Up>", move_line_up)
editor:map("n", "<A-k>", move_line_up)
editor:map("n", "<A-Down>", move_line_down)
editor:map("n", "<A-j>", move_line_down)


-- ==========================================
-- 2. VISUAL MODE: Multi-Line Block Movement
-- ==========================================
local function move_visual_block(direction)
    local c_row, c_col = editor:get_cursor()
    local a_row, a_col = editor:get_visual_anchor()

    local start_row = math.min(c_row, a_row)
    local end_row = math.max(c_row, a_row)
    local lines = editor:get_all_lines()

    if direction == "up" and start_row > 0 then
        local line_above = table.remove(lines, start_row)
        table.insert(lines, end_row + 1, line_above)

        editor:set_lines(lines)
        editor:set_selection(a_row - 1, a_col)
        editor:set_cursor(c_row - 1, c_col)
    elseif direction == "down" and end_row < #lines - 1 then
        local line_below = table.remove(lines, end_row + 2)
        table.insert(lines, start_row + 1, line_below)

        editor:set_lines(lines)
        editor:set_selection(a_row + 1, a_col)
        editor:set_cursor(c_row + 1, c_col)
    end
end

-- Map to "v" (Visual mode)
editor:map("v", "<A-Up>", function() move_visual_block("up") end)
editor:map("v", "<A-k>", function() move_visual_block("up") end)
editor:map("v", "<A-Down>", function() move_visual_block("down") end)
editor:map("v", "<A-j>", function() move_visual_block("down") end)






-- 1. Define your custom snippets dictionary
-- Values can be static strings or functions that generate dynamic text
local snippets = {
    ["date"] = function() return os.date("%Y-%m-%d") end,
    ["time"] = function() return os.date("%H:%M:%S") end,

    -- Using the editor API to get context!
    ["file-name"] = function()
        local full_path = editor:get_current_file()
        -- Extract just the filename from the path
        return full_path:match("^.+/(.+)$") or full_path
    end,

    -- A simple static string snippet
    -- ["shrug"] = "¯\\_('-')_/¯",

    -- A multi-line custom snippet
    -- ["rust-main"] = function()
    --     return "fn main() {\n    println!(\"Hello World\");\n}"
    -- end,

    -- Using system environment variables
    -- ["greeting"] = function()
    --     return "Hello, " .. (os.getenv("USER") or "User") .. "!"
    -- end
}

-- 2. Provide the list of available snippets to the UI
function on_autocomplete(trigger, query)
    local results = {}
    if trigger == "@" then
        for key, _ in pairs(snippets) do
            -- Only show snippets that match what the user typed so far
            if string.sub(key, 1, string.len(query)) == query then
                table.insert(results, key)
            end
        end
    end
    return results
end

-- 3. Provide the actual text to insert when a user selects a snippet
function expand_autocomplete(trigger, suggestion)
    if trigger == "@" then
        local action = snippets[suggestion]

        -- If it's a function, execute it to get the string
        if type(action) == "function" then
            return action()
            -- If it's just a string, return it directly
        elseif type(action) == "string" then
            return action
        end
    end

    return suggestion -- Fallback just in case
end

-- ==========================================
-- FEATURE TESTS & EXAMPLES (Future Reference)
-- ==========================================

-- ------------------------------------------
-- 1. VIRTUAL TEXT
-- ------------------------------------------
-- When you press '\vt' in normal mode, it will inject a red warning
-- note at column 40 of the line your cursor is currently on.
editor:map("n", "\\vt", function()
    local row, col = editor:get_cursor()
    -- Args: row, col, text, color (red, green, blue, yellow, gray)
    editor:set_virtual_text(row, 40, " <-- CHECK THIS", "red")
    editor:set_status("Injected virtual text at row " .. tostring(row))
end)

-- lua  editor:set_virtual_text(editor:get_cursor().row, 40, ""virtualtext", "red")
-- ------------------------------------------
-- 2. CUSTOM SEARCH UI
-- ------------------------------------------
-- Step A: Define the Data Provider (must be global so Rust can find it)
-- It takes the current string the user has typed and returns a table of matches.
_G.fruit_search_provider = function(query)
    local database = { "apple", "banana", "cherry", "date", "elderberry", "fig", "grape" }
    local results = {}

    for _, item in ipairs(database) do
        -- Simple substring match
        if query == "" or string.find(item, query) then
            table.insert(results, item)
        end
    end
    return results
end

-- Step B: Define the Selection Action (must be global)
-- This runs when the user presses Enter on an item in the search list.
_G.fruit_search_action = function(selected_item)
    -- For example, let's insert it into the file
    editor:insert_text("I love eating " .. selected_item .. "s!")
    editor:set_status("Inserted fruit: " .. selected_item)
end

-- Step C: Map a key to trigger the custom search pipeline
-- Press '\sf' to test it.
editor:map("n", "\\ip", function()
    -- Args: provider_function_name, action_function_name
    editor:start_custom_search("fruit_search_provider", "fruit_search_action")
end)


-- ------------------------------------------
-- 3. MANUAL IMAGE RENDERING
-- ------------------------------------------
-- This lets you force an image to render on the screen at a specific row,
-- even if there is no [[image.png]] markdown link there.
-- (Note: Ensure you actually have a file named "test.png" or similar in your vault)

-- Image mappings (start with \i)
editor:map("n", "\\if", function() editor:toggle_image_fullscreen() end) -- Image Fullscreen
editor:map("n", "\\is", function()                                       -- Image Show (Manual)
    local row, col = editor:get_cursor()
    local test_image = "test.jpg"
    editor:show_image(test_image, row)
end)
editor:map("n", "\\ic", function() -- Image Clear
    editor:clear_image()
    editor:set_status("Cleared image")
end)

-- ==========================================
-- DYNAMIC TEMPLATE PICKER
-- ==========================================

-- Step 1: Define the Data Provider
-- Reads the Templates directory and filters based on user input
_G.template_search_provider = function(query)
    local results = {}

    -- Use 'ls -1' for Unix/Linux/macOS or 'dir /b' for Windows.
    -- Assuming a Unix-like environment here based on your Rust scanner setup:
    local handle = io.popen('ls -1 "Templates/" 2>/dev/null')

    if handle then
        for file in handle:lines() do
            -- Only include Markdown files
            if file:match("%.md$") then
                -- Case-insensitive search match
                if query == "" or string.find(string.lower(file), string.lower(query), 1, true) then
                    table.insert(results, file)
                end
            end
        end
        handle:close()
    else
        -- Fallback if the command fails
        table.insert(results, "Yaml-Template.md")
        table.insert(results, "meeting.md")
        table.insert(results, "img-gallery.md")
    end

    return results
end

-- Step 2: Define the Selection Action
-- Triggers when you press Enter on a template in the search UI
_G.template_search_action = function(selected_item)
    if selected_item and selected_item ~= "" then
        editor:process_template("Templates/" .. selected_item)
        editor:set_status("Applied template: " .. selected_item)
    end
end

-- Step 3: Map the custom search to \nt
editor:map("n", "\\nt", function()
    editor:start_custom_search("template_search_provider", "template_search_action")
end)

-- (Optional) Keep your other specific template maps if you still want fast-tracks
editor:map("n", "\\nm", function() editor:process_template("Templates/meeting.md") end)
editor:map("n", "\\ig", function() editor:process_template("Templates/img-gallery.md") end)



-- Map sorting keys to the Table mode!
editor:map("t", "1", function() editor:sort_table(0) end)
editor:map("t", "2", function() editor:sort_table(1) end)
editor:map("t", "3", function() editor:sort_table(2) end)
editor:map("t", "4", function() editor:sort_table(3) end)
editor:map("t", "5", function() editor:sort_table(4) end)

editor:map("n", "<C-b>", function()
    editor:open_table({
        columns = { "File", "Tags", "Backlinks" },
        query = [[
            SELECT f.file_name as file,
                   GROUP_CONCAT(DISTINCT t.tag) as tags,
                   GROUP_CONCAT(DISTINCT fb.file_name) as backlinks
            FROM files f
            LEFT JOIN file_tags ft ON f.id = ft.file_id
            LEFT JOIN tags t ON ft.tag_id = t.id
            LEFT JOIN backlinks b ON f.id = b.file_id
            LEFT JOIN files fb ON fb.id = b.backlink_id
            GROUP BY f.id
        ]],
        formatter = function(row)
            local file = row[1] or ""
            local tags = row[2] or ""
            local backlinks = row[3] or ""

            return { file, tags, backlinks }
        end
    })
end)
