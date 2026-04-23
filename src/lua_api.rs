use mlua::{Function, UserData, UserDataMethods};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum EditorCommand {
    MoveToTop,
    MoveToBottom,
    Echo(String),
    Quit,
    StartSearch(String),
    // --- NEW COMMANDS ---
    YankLine,
    DeleteLine,
    EnterFileTree,
    ToggleImageFullScreen,
    StopAudio,
    ToggleReadMode,
    OpenWikilink(String),
    ProcessTemplate(String),
}

pub struct LuaEditorAPI {
    pub command_queue: Rc<RefCell<Vec<EditorCommand>>>,
    pub normal_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
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

        // Method to register keybindings from Lua
        methods.add_method(
            "map",
            |lua, this, (mode, seq, func): (String, String, Function)| {
                if mode == "n" {
                    let key = lua.create_registry_value(func)?;
                    this.normal_keymaps.borrow_mut().insert(seq, key);
                }
                Ok(())
            },
        );
    }
}
