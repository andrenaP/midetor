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
