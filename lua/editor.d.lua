---@meta

---@class Editor
editor = {}

---@param msg string
function editor:echo(msg) end

function editor:quit() end

function editor:save() end

function editor:undo() end

function editor:redo() end

function editor:move_to_top() end

function editor:yank_line() end

function editor:delete_line() end

function editor:toggle_file_tree() end

function editor:toggle_image_fullscreen() end

function editor:stop_audio() end

function editor:toggle_read_mode() end

function editor:follow_link() end

function editor:cancel() end

function editor:paste() end

function editor:insert_line_below() end

function editor:navigate_back() end

function editor:navigate_forward() end

function editor:clear_image() end

---@param path string
function editor:open_file(path) end

---@param template string
function editor:process_template(template) end

---@param search_type string
function editor:start_search(search_type) end

---@param direction "up"|"down"|"left"|"right"|"word_forward"|"word_back"|"head"|"end"|"top"|"bottom"
function editor:move(direction) end

---@param mode string
function editor:set_mode(mode) end

---@param msg string
function editor:set_status(msg) end

---@param mode "n"|"v"|"t"
---@param seq string
---@param func function
function editor:map(mode, seq, func) end

---@return number row, number col
function editor:get_cursor() end

---@return string
function editor:get_current_line() end

---@return string[]
function editor:get_all_lines() end

---@return string
function editor:get_current_file() end

---@param text string
function editor:insert_text(text) end

---@param new_lines string[]
function editor:set_lines(new_lines) end

---@param row number
---@param col number
function editor:set_cursor(row, col) end

---@return number row, number col
function editor:get_visual_anchor() end

---@param row number
---@param col number
function editor:set_selection(row, col) end

---@param row number
---@param col number
---@param text string
---@param color string
function editor:set_virtual_text(row, col, text, color) end

---@param provider string
---@param on_select string
function editor:start_custom_search(provider, on_select) end

---@param path string
---@param row number
function editor:show_image(path, row) end

function editor:open_table_browser() end

function editor:sort_table(col) end

function editor:open_table(columns, query, formatter) end
