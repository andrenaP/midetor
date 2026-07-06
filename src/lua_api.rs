use mlua::{UserData, UserDataMethods};
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// 1. The Snapshot State
#[derive(Clone, Default)]
pub struct EditorContext {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub current_file: String,
    pub visual_anchor: Option<(usize, usize)>,
}

#[derive(Clone)]
pub enum EditorCommand {
    // Basic Movement
    MoveToTop,
    MoveToBottom,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    MoveWordForward,
    MoveWordBack,
    MoveHead,
    MoveEnd,

    // Actions
    Undo,
    Redo,
    Save,
    NavigateBack,
    NavigateForward,
    EnterMode(String),
    DeleteNextChar,
    InsertLineBelow,
    Paste,

    // Existing...
    Echo(String),
    Quit,
    StartSearch(String),
    YankLine,
    DeleteLine,
    EnterFileTree,
    ToggleImageFullScreen,
    StopAudio,
    ToggleReadMode,
    OpenWikilink(String),
    ProcessTemplate(String),

    FollowLink,
    Cancel,

    ChangeStatus(String),

    // --- NEW TEXT MANIPULATION COMMANDS ---
    InsertText(String),
    SetLines(Vec<String>),
    SetCursor(usize, usize),
    SetSelection(Option<(usize, usize)>),
    SetVirtualText(usize, usize, String, String),
    ShowImage(String, usize),
    StartCustomSearch(String, String),

    OpenTableBrowser,
    OpenCustomTable {
        columns: Vec<String>,
        data: Vec<Vec<String>>, // Takes raw data directly instead of query/formatter
        on_submit_key: Option<Rc<mlua::RegistryKey>>,
        filter: Option<String>,
        sort_col: Option<usize>, // 1-based index passed from Lua
        sort_asc: Option<bool>,
    },
    SortTable(usize),

    DeleteFile(String),
    PasteImageFromClipboard(String),
}

pub struct LuaEditorAPI {
    pub command_queue: Rc<RefCell<Vec<EditorCommand>>>,
    pub normal_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
    pub visual_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
    pub table_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
    pub context: Rc<RefCell<EditorContext>>,
    pub db: Rc<RefCell<Connection>>,
}

impl UserData for LuaEditorAPI {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        // Expose basic commands
        methods.add_method("move_to_top", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::MoveToTop);
            Ok(())
        });

        methods.add_method("echo", |_, this, msg: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::Echo(msg));
            Ok(())
        });

        methods.add_method("quit", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Quit);
            Ok(())
        });

        methods.add_method("yank_line", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::YankLine);
            Ok(())
        });

        methods.add_method("delete_line", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::DeleteLine);
            Ok(())
        });

        methods.add_method("start_search", |_, this, search_type: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::StartSearch(search_type));
            Ok(())
        });

        methods.add_method("open_file", |_, this, path: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::OpenWikilink(path));
            Ok(())
        });

        methods.add_method("process_template", |_, this, template: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ProcessTemplate(template));
            Ok(())
        });

        methods.add_method("toggle_file_tree", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::EnterFileTree);
            Ok(())
        });

        methods.add_method("toggle_image_fullscreen", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ToggleImageFullScreen);
            Ok(())
        });

        methods.add_method("stop_audio", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::StopAudio);
            Ok(())
        });

        methods.add_method("toggle_read_mode", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ToggleReadMode);
            Ok(())
        });

        methods.add_method("echo", |_, this, msg: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::Echo(msg));
            Ok(())
        });
        methods.add_method("quit", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Quit);
            Ok(())
        });

        methods.add_method("follow_link", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::FollowLink);
            Ok(())
        });

        methods.add_method("cancel", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Cancel);
            Ok(())
        });

        methods.add_method("move", |_, this, direction: String| {
            let cmd = match direction.as_str() {
                "up" => EditorCommand::MoveUp,
                "down" => EditorCommand::MoveDown,
                "left" => EditorCommand::MoveLeft,
                "right" => EditorCommand::MoveRight,
                "word_forward" => EditorCommand::MoveWordForward,
                "word_back" => EditorCommand::MoveWordBack,
                "head" => EditorCommand::MoveHead,
                "end" => EditorCommand::MoveEnd,
                "top" => EditorCommand::MoveToTop,
                "bottom" => EditorCommand::MoveToBottom,
                _ => return Ok(()),
            };
            this.command_queue.borrow_mut().push(cmd);
            Ok(())
        });

        methods.add_method("undo", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Undo);
            Ok(())
        });

        methods.add_method("redo", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Redo);
            Ok(())
        });

        methods.add_method("save", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Save);
            Ok(())
        });

        methods.add_method("set_mode", |_, this, mode: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::EnterMode(mode));
            Ok(())
        });

        methods.add_method("paste", |_, this, ()| {
            this.command_queue.borrow_mut().push(EditorCommand::Paste);
            Ok(())
        });

        methods.add_method("insert_line_below", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::InsertLineBelow);
            Ok(())
        });

        methods.add_method("navigate_back", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::NavigateBack);
            Ok(())
        });

        methods.add_method("navigate_forward", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::NavigateForward);
            Ok(())
        });

        // This just changes the text at the bottom, without changing the view!
        methods.add_method("set_status", |_, this, msg: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ChangeStatus(msg));
            Ok(())
        });

        // ==========================================
        // READ METHODS (Lua -> Rust State)
        // ==========================================

        methods.add_method("get_cursor", |_, this, ()| {
            let ctx = this.context.borrow();
            Ok((ctx.cursor_row, ctx.cursor_col)) // Returns a tuple to Lua
        });

        methods.add_method("get_current_line", |_, this, ()| {
            let ctx = this.context.borrow();
            if let Some(line) = ctx.lines.get(ctx.cursor_row) {
                Ok(line.clone())
            } else {
                Ok(String::new())
            }
        });

        methods.add_method("get_all_lines", |_, this, ()| {
            let ctx = this.context.borrow();
            Ok(ctx.lines.clone()) // Automatically converts to a Lua Table of strings!
        });

        methods.add_method("get_current_file", |_, this, ()| {
            let ctx = this.context.borrow();
            Ok(ctx.current_file.clone())
        });

        // ==========================================
        // WRITE METHODS (Lua -> Command Queue)
        // ==========================================

        methods.add_method("insert_text", |_, this, text: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::InsertText(text));
            Ok(())
        });

        // Replaces the entire buffer (like vim.api.nvim_buf_set_lines)
        methods.add_method("set_lines", |_, this, new_lines: Vec<String>| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::SetLines(new_lines));
            Ok(())
        });

        // Jump to a specific row and col
        methods.add_method("set_cursor", |_, this, (row, col): (usize, usize)| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::SetCursor(row, col));
            Ok(())
        });

        methods.add_method(
            "map",
            |lua, this, (mode, seq, func): (String, String, mlua::Function)| {
                let key = lua.create_registry_value(func)?;
                match mode.as_str() {
                    "n" => {
                        this.normal_keymaps.borrow_mut().insert(seq, key);
                    }
                    "v" => {
                        this.visual_keymaps.borrow_mut().insert(seq, key);
                    }
                    "t" => {
                        this.table_keymaps.borrow_mut().insert(seq, key);
                    } // NEW
                    _ => {}
                }
                Ok(())
            },
        );

        // Gets the anchor coordinates
        methods.add_method("get_visual_anchor", |_, this, ()| {
            let ctx = this.context.borrow();
            if let Some(anchor) = ctx.visual_anchor {
                Ok((anchor.0, anchor.1))
            } else {
                Ok((ctx.cursor_row, ctx.cursor_col)) // Fallback if no anchor
            }
        });

        // Sets the anchor coordinates
        methods.add_method("set_selection", |_, this, (row, col): (usize, usize)| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::SetSelection(Some((row, col))));
            Ok(())
        });
        methods.add_method(
            "set_virtual_text",
            |_, this, (row, col, text, color): (usize, usize, String, String)| {
                this.command_queue
                    .borrow_mut()
                    .push(EditorCommand::SetVirtualText(row, col, text, color));
                Ok(())
            },
        );
        methods.add_method("start_custom_search", |_, this, callback_name: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::StartSearch(format!("lua:{}", callback_name)));
            Ok(())
        });

        methods.add_method(
            "start_custom_search",
            |_, this, (provider, on_select): (String, String)| {
                this.command_queue
                    .borrow_mut()
                    .push(EditorCommand::StartCustomSearch(provider, on_select));
                Ok(())
            },
        );
        methods.add_method("show_image", |_, this, (path, row): (String, usize)| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ShowImage(path, row));
            Ok(())
        });

        methods.add_method("clear_image", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::ChangeStatus(
                    "clearing_image_flag".to_string(),
                ));
            Ok(())
        });
        methods.add_method("open_table_browser", |_, this, ()| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::OpenTableBrowser);
            Ok(())
        });

        // Trigger Sorting
        methods.add_method("sort_table", |_, this, col: usize| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::SortTable(col));
            Ok(())
        });

        // Execute Raw SQL Query directly returning (columns, row_data)
        methods.add_method("query_db", |_, this, query: String| {
            let db = this.db.borrow();
            let mut stmt = db
                .prepare(&query)
                .map_err(|e| mlua::Error::RuntimeError(format!("SQL Prepare Error: {}", e)))?;

            let column_count = stmt.column_count();
            let col_names: Vec<String> =
                stmt.column_names().into_iter().map(String::from).collect();

            let mut raw_rows = Vec::new();
            let mut rows_iter = stmt
                .query([])
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

            while let Some(row) = rows_iter
                .next()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
            {
                let mut row_data = Vec::new();
                for i in 0..column_count {
                    let val: String =
                        match row.get_ref(i).unwrap_or(rusqlite::types::ValueRef::Null) {
                            rusqlite::types::ValueRef::Null => String::new(),
                            rusqlite::types::ValueRef::Integer(num) => num.to_string(),
                            rusqlite::types::ValueRef::Real(num) => num.to_string(),
                            rusqlite::types::ValueRef::Text(t) => {
                                String::from_utf8_lossy(t).to_string()
                            }
                            rusqlite::types::ValueRef::Blob(b) => {
                                String::from_utf8_lossy(b).to_string()
                            }
                        };
                    row_data.push(val);
                }
                raw_rows.push(row_data);
            }
            Ok((col_names, raw_rows))
        });

        // Simplified table opening - receives data pre-processed
        // Inside `add_methods` for `LuaEditorAPI`:
        methods.add_method("open_table", |lua, this, config: mlua::Table| {
            let columns: Vec<String> = config.get("columns")?;
            let data: Vec<Vec<String>> = config.get("data")?;
            let filter: Option<String> = config.get("filter")?;
            let sort_col: Option<usize> = config.get("sort_col")?;
            let sort_asc: Option<bool> = config.get("sort_asc")?;
            let on_submit: Option<mlua::Function> = config.get("on_submit").ok();

            let on_submit_key = if let Some(f) = on_submit {
                Some(Rc::new(lua.create_registry_value(f)?))
            } else {
                None
            };

            this.command_queue
                .borrow_mut()
                .push(EditorCommand::OpenCustomTable {
                    columns,
                    data,
                    on_submit_key,
                    filter,
                    sort_col,
                    sort_asc,
                });
            Ok(())
        });

        methods.add_method("delete_file", |_, this, path: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::DeleteFile(path));
            Ok(())
        });
        methods.add_method("paste_image_from_clipboard", |_, this, path: String| {
            this.command_queue
                .borrow_mut()
                .push(EditorCommand::PasteImageFromClipboard(path));
            Ok(())
        });
    }
}
