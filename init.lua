-- init.lua
-- Normal Mode Mappings

editor:map("n", "gg", function() editor:move_to_top() end)
editor:map("n", "yy", function() editor:yank_line() end)
editor:map("n", "dd", function() editor:delete_line() end)

-- Searching
editor:map("n", "\\ob", function() editor:start_search("backlinks") end)
editor:map("n", "\\ot", function() editor:start_search("tags") end)
editor:map("n", "\\f",  function() editor:start_search("files") end)
editor:map("n", "\\os", function() editor:start_search("sql") end)

-- File Tree
editor:map("n", "\\t", function() editor:toggle_file_tree() end)

-- UI / Modes
editor:map("n", "\\if", function() editor:toggle_image_fullscreen() end)
editor:map("n", "\\s",  function() editor:stop_audio() end)
editor:map("n", "\\w",  function() editor:toggle_read_mode() end)

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
