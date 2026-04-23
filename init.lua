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
    editor:open_file(today)
end)

editor:map("n", "\\ooy", function()
    -- 86400 seconds = 1 day
    local yesterday = os.date("Every day info/%Y-%m-%d.md", os.time() - 86400)
    editor:open_file(yesterday)
end)

editor:map("n", "\\ooT", function()
    local tomorrow = os.date("Every day info/%Y-%m-%d.md", os.time() + 86400)
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
editor:map("n", "<Esc>", function() editor:echo("Normal Mode") end)


-- Add these to your init.lua file:

-- The arrow keys should now work with the fixed event_to_string!
editor:map("n", "<Left>", function() editor:move("left") end)
editor:map("n", "<Right>", function() editor:move("right") end)
editor:map("n", "<Up>", function() editor:move("up") end)
editor:map("n", "<Down>", function() editor:move("down") end)

-- Handle Enter (Links/Audio) and Escape (Fullscreen closing)
editor:map("n", "<Enter>", function() editor:follow_link() end)
editor:map("n", "<Esc>", function()
    editor:cancel()
    editor:echo("Normal Mode")
end)


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

-- Fixed Escape Mapping!
editor:map("n", "<Esc>", function()
    editor:cancel()
    editor:set_status("Normal Mode")
end)


function run_bash_script_and_save()
    -- editor:save()
    _G.Obsidian_valt_main_path = os.getenv("Obsidian_valt_main_path")
    local current_file_path = editor:get_current_file()
    local script_path = 'markdown-scanner "' ..
        current_file_path .. '" "' .. _G.Obsidian_valt_main_path .. '" --json-only '

    local handle = io.popen(script_path)
    local result = handle:read("*a")
    handle:close()

    editor:echo(result .. " THIS IS RUST BABY")
end

editor:map("n", "<C-y>", function() run_bash_script_and_save() end)


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



local function move_line_up()
    local row, col = editor:get_cursor()
    if row > 0 then
        local lines = editor:get_all_lines()
        lines[row + 1], lines[row] = lines[row], lines[row + 1] -- Lua shortcut for swapping!
        editor:set_lines(lines)
        editor:set_cursor(row - 1, col)
    end
end

local function move_line_down()
    local row, col = editor:get_cursor()
    local lines = editor:get_all_lines()
    if row < #lines - 1 then
        lines[row + 1], lines[row + 2] = lines[row + 2], lines[row + 1]
        editor:set_lines(lines)
        editor:set_cursor(row + 1, col)
    end
end

editor:map("n", "<A-Up>", move_line_up)
editor:map("n", "<A-k>", move_line_up)

editor:map("n", "<A-Down>", move_line_down)
editor:map("n", "<A-j>", move_line_down)
