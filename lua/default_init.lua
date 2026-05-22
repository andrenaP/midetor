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



local function open_dir_table(dir_path)
    local cmd = string.format('ls -lh --time-style=+"%%Y-%%m-%%d" "%s"', dir_path)
    local handle = io.popen(cmd)

    if not handle then
        editor:echo("Failed to execute ls")
        return
    end

    local result = handle:read("*a")
    handle:close()

    local formatted_data = {}

    if dir_path ~= "." and dir_path ~= "" then
        local parent = dir_path:match("^(.*)/[^/]+$") or "."
        -- Changed 'true' to '"true"' (String)
        table.insert(formatted_data, { "📁 ..", "", "", "drwxr-xr-x", "", "..", parent, "true" })
    end

    for line in result:gmatch("[^\r\n]+") do
        if not line:match("^total") then
            local perms, links, owner, group, size, date_str, name =
                line:match("^(%S+)%s+(%S+)%s+(%S+)%s+(%S+)%s+(%S+)%s+(%S+)%s+(.+)$")

            if name then
                local is_dir = (perms:sub(1, 1) == "d")
                local icon = is_dir and "📁" or "📄"
                local display_name = icon .. " " .. name

                local full_path = (dir_path == ".") and name or (dir_path .. "/" .. name)

                -- Convert the boolean to a string before inserting
                local dir_flag = is_dir and "true" or "false"

                table.insert(formatted_data, {
                    display_name, size, date_str, perms, owner,
                    name, full_path, dir_flag
                })
            end
        end
    end

    editor:open_table({
        columns = { "Name", "Size", "Date", "Permissions", "Owner" },
        data = formatted_data,
        on_submit = function(row)
            local full_path = row[7]
            -- Evaluate the string back into a boolean
            local is_dir = (row[8] == "true")

            if is_dir then
                open_dir_table(full_path)
            else
                if string.find(full_path, ".md") then
                    editor:open_file(full_path)
                else
                    editor:echo("Can't open: " .. full_path)
                end
            end
        end
    })
end

editor:map("n", "<C-l>", function()
    open_dir_table(".")
end)
