use mlua::{Function, UserData, UserDataMethods};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
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
    }
}
