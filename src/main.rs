#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("LayoutFixer works on Windows only.");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = app::run() {
        app::show_error(&error);
    }
}

#[cfg(windows)]
mod app {
    use std::env;
    use std::ffi::c_void;
    use std::fs;
    use std::io::{ErrorKind, Write};
    use std::mem::{size_of, zeroed};
    use std::path::PathBuf;
    use std::ptr::{copy_nonoverlapping, null_mut};
    use std::thread::sleep;
    use std::time::Duration;

    type Bool = i32;
    type Dword = u32;
    type Uint = u32;
    type Word = u16;
    type Wparam = usize;
    type Lparam = isize;
    type Handle = *mut c_void;
    type Hwnd = *mut c_void;
    type Hglobal = *mut c_void;

    const CONVERT_HOTKEY_ID: i32 = 1;
    const EXIT_HOTKEY_ID: i32 = 2;
    const WM_HOTKEY: Uint = 0x0312;
    const CONFIG_FILE_NAME: &str = "layout_fixer.cfg";
    const INSTANCE_MUTEX_NAME: &str = "Local\\LayoutFixerSingleInstance";
    const ERROR_ALREADY_EXISTS: Dword = 183;

    const MOD_ALT: Uint = 0x0001;
    const MOD_CONTROL: Uint = 0x0002;
    const MOD_SHIFT: Uint = 0x0004;
    const MOD_WIN: Uint = 0x0008;
    const MOD_NOREPEAT: Uint = 0x4000;

    const MB_OK: Uint = 0x0000;
    const MB_ICONERROR: Uint = 0x0010;
    const MB_ICONINFORMATION: Uint = 0x0040;

    const CF_UNICODETEXT: Uint = 13;
    const GMEM_MOVEABLE: Uint = 0x0002;
    const GMEM_ZEROINIT: Uint = 0x0040;

    const INPUT_KEYBOARD: Dword = 1;
    const KEYEVENTF_KEYUP: Dword = 0x0002;
    const VK_CONTROL: Word = 0x11;
    const VK_SHIFT: Word = 0x10;
    const VK_MENU: Word = 0x12;
    const VK_LWIN: Word = 0x5B;
    const VK_RWIN: Word = 0x5C;
    const VK_C: Word = 0x43;
    const VK_V: Word = 0x56;
    const VK_F12: Uint = 0x7B;

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: Uint,
        w_param: Wparam,
        l_param: Lparam,
        time: Dword,
        pt: Point,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        mouse_data: Dword,
        dw_flags: Dword,
        time: Dword,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        w_vk: Word,
        w_scan: Word,
        dw_flags: Dword,
        time: Dword,
        dw_extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct HardwareInput {
        u_msg: Dword,
        w_param_l: Word,
        w_param_h: Word,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    union InputUnion {
        mi: MouseInput,
        ki: KeybdInput,
        hi: HardwareInput,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Input {
        input_type: Dword,
        u: InputUnion,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: Hwnd, id: i32, fs_modifiers: Uint, vk: Uint) -> Bool;
        fn UnregisterHotKey(hwnd: Hwnd, id: i32) -> Bool;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: Uint, max: Uint) -> Bool;
        fn GetAsyncKeyState(vkey: i32) -> i16;
        fn MessageBoxW(hwnd: Hwnd, text: *const u16, caption: *const u16, flags: Uint) -> i32;
        fn SendInput(count: Uint, inputs: *const Input, size: i32) -> Uint;
        fn OpenClipboard(hwnd: Hwnd) -> Bool;
        fn CloseClipboard() -> Bool;
        fn EmptyClipboard() -> Bool;
        fn GetClipboardData(format: Uint) -> Handle;
        fn SetClipboardData(format: Uint, mem: Handle) -> Handle;
        fn GetClipboardSequenceNumber() -> Dword;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(
            security_attributes: Handle,
            initial_owner: Bool,
            name: *const u16,
        ) -> Handle;
        fn CloseHandle(object: Handle) -> Bool;
        fn GetLastError() -> Dword;
        fn GlobalAlloc(flags: Uint, bytes: usize) -> Hglobal;
        fn GlobalFree(mem: Hglobal) -> Hglobal;
        fn GlobalLock(mem: Hglobal) -> *mut c_void;
        fn GlobalUnlock(mem: Hglobal) -> Bool;
        fn GlobalSize(mem: Hglobal) -> usize;
    }

    pub fn run() -> Result<(), String> {
        let _single_instance = match acquire_single_instance()? {
            Some(guard) => guard,
            None => return Ok(()),
        };

        let config_path = config_path();
        let (hotkey, configured_now) = load_or_capture_hotkey(&config_path)?;
        register_exit_hotkey()?;

        if configured_now {
            show_info(&format!(
                "Registered: {}\nSaved to: {}\nExit: Ctrl+Alt+F12\n\nThe app is now running in the background.",
                hotkey.display,
                config_path.display()
            ));
        }

        unsafe {
            let mut msg: Msg = zeroed();
            loop {
                let result = GetMessageW(&mut msg, null_mut(), 0, 0);
                if result == -1 {
                    break;
                }
                if result == 0 {
                    break;
                }
                if msg.message == WM_HOTKEY {
                    match msg.w_param as i32 {
                        CONVERT_HOTKEY_ID => {
                            wait_for_hotkey_release(&hotkey);
                            let _ = convert_selection();
                        }
                        EXIT_HOTKEY_ID => break,
                        _ => {}
                    }
                }
            }
            UnregisterHotKey(null_mut(), CONVERT_HOTKEY_ID);
            UnregisterHotKey(null_mut(), EXIT_HOTKEY_ID);
        }

        Ok(())
    }

    struct SingleInstanceGuard {
        handle: Handle,
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    fn acquire_single_instance() -> Result<Option<SingleInstanceGuard>, String> {
        let name = to_wide_null(INSTANCE_MUTEX_NAME);
        let handle = unsafe { CreateMutexW(null_mut(), 1, name.as_ptr()) };

        if handle.is_null() {
            return Err(format!(
                "Could not create single-instance lock. {}",
                last_error()
            ));
        }

        let error = unsafe { GetLastError() };
        if error == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            return Ok(None);
        }

        Ok(Some(SingleInstanceGuard { handle }))
    }

    fn load_or_capture_hotkey(config_path: &PathBuf) -> Result<(Hotkey, bool), String> {
        match load_hotkey_config(config_path) {
            Ok(Some(hotkey)) if is_exit_hotkey(&hotkey) => {
                show_error("Saved hotkey is Ctrl+Alt+F12, which is reserved for exit.");
            }
            Ok(Some(hotkey)) => {
                if register_hotkey(&hotkey) {
                    return Ok((hotkey, false));
                }
                show_error(&format!(
                    "Saved hotkey ({}) is already used by another app.\n{}\n\nPress OK, then choose another shortcut.",
                    hotkey.display,
                    last_error()
                ));
            }
            Ok(None) => {}
            Err(error) => {
                show_error(&format!(
                    "Could not read {}.\n{}\n\nPress OK, then choose a shortcut.",
                    config_path.display(),
                    error
                ));
            }
        }

        let hotkey = capture_and_save_hotkey(config_path)?;
        Ok((hotkey, true))
    }

    fn capture_and_save_hotkey(config_path: &PathBuf) -> Result<Hotkey, String> {
        show_info(
            "Press OK, then press the shortcut you want to use and release it.\n\nExamples:\nCtrl+Alt+T\nF8\nCtrl+Shift+Space",
        );

        loop {
            match capture_hotkey() {
                Ok(hotkey) => {
                    if is_exit_hotkey(&hotkey) {
                        show_error("Ctrl+Alt+F12 is reserved for exit.\n\nPress OK, then choose another shortcut.");
                        continue;
                    }
                    if register_hotkey(&hotkey) {
                        save_hotkey_config(config_path, &hotkey)?;
                        return Ok(hotkey);
                    }
                    show_error(&format!(
                        "Hotkey is already used by another app.\n{}\n\nPress OK, then choose another shortcut.",
                        last_error()
                    ));
                }
                Err(error) => {
                    show_error(&format!(
                        "{error}\n\nPress OK, then choose another shortcut."
                    ));
                }
            }
        }
    }

    fn show_info(message: &str) {
        message_box("LayoutFixer", message, MB_OK | MB_ICONINFORMATION);
    }

    pub fn show_error(message: &str) {
        message_box("LayoutFixer", message, MB_OK | MB_ICONERROR);
    }

    fn message_box(title: &str, message: &str, flags: Uint) {
        let title = to_wide_null(title);
        let message = to_wide_null(message);
        unsafe {
            MessageBoxW(null_mut(), message.as_ptr(), title.as_ptr(), flags);
        }
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        let mut wide: Vec<u16> = value.encode_utf16().collect();
        wide.push(0);
        wide
    }

    fn config_path() -> PathBuf {
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                return parent.join(CONFIG_FILE_NAME);
            }
        }

        PathBuf::from(CONFIG_FILE_NAME)
    }

    fn load_hotkey_config(path: &PathBuf) -> Result<Option<Hotkey>, String> {
        if !path.exists() {
            return Ok(None);
        }

        parse_hotkey_config(&fs::read_to_string(path).map_err(|error| error.to_string())?).map(Some)
    }

    fn save_hotkey_config(path: &PathBuf, hotkey: &Hotkey) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        let content = format!(
            "# LayoutFixer hotkey config\n# Exit hotkey is always Ctrl+Alt+F12\n\
display={}\nmodifiers={}\nvk={}\n",
            hotkey.display, hotkey.modifiers, hotkey.vk
        );

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => file.write_all(content.as_bytes()).map_err(|error| {
                format!(
                    "Could not save hotkey config to {}.\n{}",
                    path.display(),
                    error
                )
            }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!(
                "Could not save hotkey config to {}.\n{}",
                path.display(),
                error
            )),
        }
    }

    fn parse_hotkey_config(content: &str) -> Result<Hotkey, String> {
        let mut display = None;
        let mut modifiers = None;
        let mut vk = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("Invalid config line: {line}"));
            };

            match key.trim() {
                "display" => display = Some(value.trim().to_string()),
                "modifiers" => {
                    modifiers = Some(
                        value
                            .trim()
                            .parse::<Uint>()
                            .map_err(|_| "Invalid modifiers value.".to_string())?,
                    );
                }
                "vk" => {
                    vk = Some(
                        value
                            .trim()
                            .parse::<Uint>()
                            .map_err(|_| "Invalid vk value.".to_string())?,
                    );
                }
                _ => {}
            }
        }

        let modifiers = modifiers.ok_or_else(|| "Missing modifiers in config.".to_string())?;
        let vk = vk.ok_or_else(|| "Missing vk in config.".to_string())?;
        let display = display.ok_or_else(|| "Missing display in config.".to_string())?;

        if display.is_empty() {
            return Err("Empty display in config.".to_string());
        }

        if modifiers & !(MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_WIN) != 0 {
            return Err("Invalid modifier bits in config.".to_string());
        }

        if vk == 0 {
            return Err("Invalid vk in config.".to_string());
        }

        Ok(Hotkey {
            modifiers,
            vk,
            display,
        })
    }

    fn register_hotkey(hotkey: &Hotkey) -> bool {
        unsafe {
            RegisterHotKey(
                null_mut(),
                CONVERT_HOTKEY_ID,
                hotkey.modifiers | MOD_NOREPEAT,
                hotkey.vk,
            ) != 0
        }
    }

    fn register_exit_hotkey() -> Result<(), String> {
        let registered = unsafe {
            RegisterHotKey(
                null_mut(),
                EXIT_HOTKEY_ID,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                VK_F12,
            )
        };

        if registered == 0 {
            return Err(format!(
                "Could not register Ctrl+Alt+F12 as exit hotkey. {}",
                last_error()
            ));
        }

        Ok(())
    }

    fn is_exit_hotkey(hotkey: &Hotkey) -> bool {
        hotkey.vk == VK_F12 && hotkey.modifiers == (MOD_CONTROL | MOD_ALT)
    }

    fn capture_hotkey() -> Result<Hotkey, String> {
        wait_until_supported_keys_are_up();

        let mut captured_modifiers = 0;
        let mut captured_key = None;
        let mut saw_any_key = false;
        let mut release_polls = 0;

        loop {
            let state = current_hotkey_state();
            if state.any_key_down {
                saw_any_key = true;
                release_polls = 0;
                captured_modifiers |= state.modifiers;
                if let Some(key) = state.primary_key {
                    captured_key = Some(key);
                }
            } else if saw_any_key {
                release_polls += 1;
                if release_polls >= 4 {
                    break;
                }
            }

            sleep(Duration::from_millis(25));
        }

        let (vk, key_display) =
            captured_key.ok_or_else(|| "Shortcut needs one non-modifier key.".to_string())?;

        if captured_modifiers == 0 && is_letter_or_digit(vk) {
            return Err(
                "Letters and numbers need Ctrl, Alt, Shift, or Win. F-keys can be used alone."
                    .to_string(),
            );
        }

        Ok(Hotkey {
            modifiers: captured_modifiers,
            vk,
            display: format_hotkey(captured_modifiers, &key_display),
        })
    }

    fn wait_until_supported_keys_are_up() {
        for _ in 0..40 {
            if !current_hotkey_state().any_key_down {
                break;
            }
            sleep(Duration::from_millis(25));
        }
    }

    struct HotkeyState {
        modifiers: Uint,
        primary_key: Option<(Uint, String)>,
        any_key_down: bool,
    }

    fn current_hotkey_state() -> HotkeyState {
        let mut modifiers = 0;

        if is_key_down(VK_CONTROL) {
            modifiers |= MOD_CONTROL;
        }
        if is_key_down(VK_SHIFT) {
            modifiers |= MOD_SHIFT;
        }
        if is_key_down(VK_MENU) {
            modifiers |= MOD_ALT;
        }
        if is_key_down(VK_LWIN) || is_key_down(VK_RWIN) {
            modifiers |= MOD_WIN;
        }

        let primary_key = current_primary_key();
        let any_key_down = modifiers != 0 || primary_key.is_some();

        HotkeyState {
            modifiers,
            primary_key,
            any_key_down,
        }
    }

    fn current_primary_key() -> Option<(Uint, String)> {
        for vk in 0x30..=0x39 {
            if is_key_down(vk as Word) {
                let ch = char::from_u32(vk).unwrap();
                return Some((vk, ch.to_string()));
            }
        }

        for vk in 0x41..=0x5A {
            if is_key_down(vk as Word) {
                let ch = char::from_u32(vk).unwrap();
                return Some((vk, ch.to_string()));
            }
        }

        for number in 1..=24 {
            let vk = 0x70 + number - 1;
            if is_key_down(vk as Word) {
                return Some((vk, format!("F{number}")));
            }
        }

        for (vk, display) in special_keys() {
            if is_key_down(*vk) {
                return Some((*vk as Uint, (*display).to_string()));
            }
        }

        None
    }

    fn special_keys() -> &'static [(Word, &'static str)] {
        &[
            (0x20, "Space"),
            (0x09, "Tab"),
            (0x0D, "Enter"),
            (0x1B, "Esc"),
            (0x2D, "Insert"),
            (0x2E, "Delete"),
            (0x24, "Home"),
            (0x23, "End"),
            (0x21, "PageUp"),
            (0x22, "PageDown"),
            (0x25, "Left"),
            (0x26, "Up"),
            (0x27, "Right"),
            (0x28, "Down"),
        ]
    }

    fn is_letter_or_digit(vk: Uint) -> bool {
        (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk)
    }

    fn format_hotkey(modifiers: Uint, key: &str) -> String {
        let mut parts = Vec::new();
        if modifiers & MOD_CONTROL != 0 {
            parts.push("Ctrl");
        }
        if modifiers & MOD_ALT != 0 {
            parts.push("Alt");
        }
        if modifiers & MOD_SHIFT != 0 {
            parts.push("Shift");
        }
        if modifiers & MOD_WIN != 0 {
            parts.push("Win");
        }
        parts.push(key);
        parts.join("+")
    }

    fn wait_for_hotkey_release(hotkey: &Hotkey) {
        let mut keys = Vec::with_capacity(6);
        keys.push(hotkey.vk as Word);

        if hotkey.modifiers & MOD_CONTROL != 0 {
            keys.push(VK_CONTROL);
        }
        if hotkey.modifiers & MOD_SHIFT != 0 {
            keys.push(VK_SHIFT);
        }
        if hotkey.modifiers & MOD_ALT != 0 {
            keys.push(VK_MENU);
        }
        if hotkey.modifiers & MOD_WIN != 0 {
            keys.push(VK_LWIN);
            keys.push(VK_RWIN);
        }

        for _ in 0..40 {
            if keys.iter().all(|&key| !is_key_down(key)) {
                break;
            }
            sleep(Duration::from_millis(25));
        }
    }

    fn is_key_down(vk: Word) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) < 0 }
    }

    struct Hotkey {
        modifiers: Uint,
        vk: Uint,
        display: String,
    }

    fn convert_selection() -> Result<&'static str, String> {
        let before = unsafe { GetClipboardSequenceNumber() };
        send_copy()?;

        let mut changed = false;
        for _ in 0..20 {
            sleep(Duration::from_millis(25));
            if unsafe { GetClipboardSequenceNumber() } != before {
                changed = true;
                break;
            }
        }

        if !changed {
            return Err(
                "No text was copied. Select text first, or the current app blocked Ctrl+C."
                    .to_string(),
            );
        }

        let selected =
            get_clipboard_text()?.ok_or_else(|| "Clipboard does not contain text.".to_string())?;
        if selected.is_empty() {
            return Err("Selected text is empty.".to_string());
        }

        let (converted, direction) = convert_auto(&selected);
        if converted == selected {
            return Err("No mapped letters found in the selected text.".to_string());
        }

        set_clipboard_text(&converted)?;
        send_paste()?;
        Ok(direction)
    }

    fn send_copy() -> Result<(), String> {
        send_ctrl_key(VK_C)
    }

    fn send_paste() -> Result<(), String> {
        send_ctrl_key(VK_V)
    }

    fn send_ctrl_key(key: Word) -> Result<(), String> {
        let inputs = [
            keyboard_input(VK_CONTROL, false),
            keyboard_input(key, false),
            keyboard_input(key, true),
            keyboard_input(VK_CONTROL, true),
        ];

        let sent = unsafe {
            SendInput(
                inputs.len() as Uint,
                inputs.as_ptr(),
                size_of::<Input>() as i32,
            )
        };
        if sent != inputs.len() as Uint {
            return Err(format!("SendInput failed. {}", last_error()));
        }
        Ok(())
    }

    fn keyboard_input(vk: Word, key_up: bool) -> Input {
        Input {
            input_type: INPUT_KEYBOARD,
            u: InputUnion {
                ki: KeybdInput {
                    w_vk: vk,
                    w_scan: 0,
                    dw_flags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                    time: 0,
                    dw_extra_info: 0,
                },
            },
        }
    }

    struct ClipboardGuard;

    impl ClipboardGuard {
        fn open() -> Result<Self, String> {
            for _ in 0..10 {
                if unsafe { OpenClipboard(null_mut()) } != 0 {
                    return Ok(Self);
                }
                sleep(Duration::from_millis(20));
            }
            Err(format!("OpenClipboard failed. {}", last_error()))
        }
    }

    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    fn get_clipboard_text() -> Result<Option<String>, String> {
        let _clipboard = ClipboardGuard::open()?;
        let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
        if handle.is_null() {
            return Ok(None);
        }

        let ptr = unsafe { GlobalLock(handle as Hglobal) } as *const u16;
        if ptr.is_null() {
            return Err(format!("GlobalLock failed. {}", last_error()));
        }

        let size = unsafe { GlobalSize(handle as Hglobal) } / size_of::<u16>();
        let mut len = 0;
        while len < size && unsafe { *ptr.add(len) } != 0 {
            len += 1;
        }

        let text = unsafe {
            let slice = std::slice::from_raw_parts(ptr, len);
            String::from_utf16_lossy(slice)
        };
        unsafe {
            GlobalUnlock(handle as Hglobal);
        }

        Ok(Some(text))
    }

    fn set_clipboard_text(text: &str) -> Result<(), String> {
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        wide.push(0);
        let bytes = wide.len() * size_of::<u16>();

        let _clipboard = ClipboardGuard::open()?;
        if unsafe { EmptyClipboard() } == 0 {
            return Err(format!("EmptyClipboard failed. {}", last_error()));
        }

        let mem = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes) };
        if mem.is_null() {
            return Err(format!("GlobalAlloc failed. {}", last_error()));
        }

        let ptr = unsafe { GlobalLock(mem) } as *mut u16;
        if ptr.is_null() {
            unsafe {
                GlobalFree(mem);
            }
            return Err(format!("GlobalLock failed. {}", last_error()));
        }

        unsafe {
            copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            GlobalUnlock(mem);
        }

        if unsafe { SetClipboardData(CF_UNICODETEXT, mem as Handle) }.is_null() {
            unsafe {
                GlobalFree(mem);
            }
            return Err(format!("SetClipboardData failed. {}", last_error()));
        }

        Ok(())
    }

    fn last_error() -> String {
        format!("Win32 error {}", unsafe { GetLastError() })
    }

    fn convert_auto(text: &str) -> (String, &'static str) {
        let en_hits = text.chars().filter(|&ch| is_en_mapped(ch)).count();
        let ar_hits = text
            .chars()
            .filter(|&ch| ar_to_en_char(ch).is_some())
            .count();

        if ar_hits > en_hits {
            (convert_ar_to_en(text), "AR -> EN")
        } else {
            (convert_en_to_ar(text), "EN -> AR")
        }
    }

    fn is_en_mapped(ch: char) -> bool {
        if ch.is_ascii_alphanumeric() {
            return ch.is_ascii_alphabetic();
        }

        matches!(
            ch,
            '`' | '[' | ']' | ';' | '\'' | ',' | '.' | '/' | '?' | ' '
        )
    }

    fn convert_en_to_ar(text: &str) -> String {
        let uppercase_count = text.chars().filter(|ch| ch.is_ascii_uppercase()).count();
        let lowercase_count = text.chars().filter(|ch| ch.is_ascii_lowercase()).count();
        let caps_mode = uppercase_count >= 2 && lowercase_count == 0;

        let mut output = String::with_capacity(text.len());
        for ch in text.chars() {
            if let Some(mapped) = en_to_ar_char(ch, caps_mode) {
                output.push_str(mapped);
            } else {
                output.push(ch);
            }
        }
        output
    }

    fn convert_ar_to_en(text: &str) -> String {
        let mut output = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == 'ل' {
                if let Some(next) = chars.peek().copied() {
                    let mapped = match next {
                        'ا' => Some("b"),
                        'أ' => Some("G"),
                        'إ' => Some("T"),
                        'آ' => Some("B"),
                        _ => None,
                    };

                    if let Some(mapped) = mapped {
                        chars.next();
                        output.push_str(mapped);
                        continue;
                    }
                }
            }

            if let Some(mapped) = ar_to_en_char(ch) {
                output.push_str(mapped);
            } else {
                output.push(ch);
            }
        }

        output
    }

    fn en_to_ar_char(ch: char, caps_mode: bool) -> Option<&'static str> {
        if caps_mode && ch.is_ascii_alphabetic() {
            return en_to_ar_base(ch.to_ascii_lowercase());
        }

        match ch {
            'H' => Some("أ"),
            'Y' => Some("إ"),
            'N' => Some("آ"),
            'G' => Some("لأ"),
            'T' => Some("لإ"),
            'B' => Some("لآ"),
            'P' => Some("؛"),
            'K' => Some("،"),
            '?' => Some("؟"),
            _ if ch.is_ascii_alphabetic() => en_to_ar_base(ch.to_ascii_lowercase()),
            _ => en_to_ar_base(ch),
        }
    }

    fn en_to_ar_base(ch: char) -> Option<&'static str> {
        match ch {
            '`' => Some("ذ"),
            'q' => Some("ض"),
            'w' => Some("ص"),
            'e' => Some("ث"),
            'r' => Some("ق"),
            't' => Some("ف"),
            'y' => Some("غ"),
            'u' => Some("ع"),
            'i' => Some("ه"),
            'o' => Some("خ"),
            'p' => Some("ح"),
            '[' => Some("ج"),
            ']' => Some("د"),
            'a' => Some("ش"),
            's' => Some("س"),
            'd' => Some("ي"),
            'f' => Some("ب"),
            'g' => Some("ل"),
            'h' => Some("ا"),
            'j' => Some("ت"),
            'k' => Some("ن"),
            'l' => Some("م"),
            ';' => Some("ك"),
            '\'' => Some("ط"),
            'z' => Some("ئ"),
            'x' => Some("ء"),
            'c' => Some("ؤ"),
            'v' => Some("ر"),
            'b' => Some("لا"),
            'n' => Some("ى"),
            'm' => Some("ة"),
            ',' => Some("و"),
            '.' => Some("ز"),
            '/' => Some("ظ"),
            _ => None,
        }
    }

    fn ar_to_en_char(ch: char) -> Option<&'static str> {
        match ch {
            'ذ' => Some("`"),
            'ض' => Some("q"),
            'ص' => Some("w"),
            'ث' => Some("e"),
            'ق' => Some("r"),
            'ف' => Some("t"),
            'غ' => Some("y"),
            'ع' => Some("u"),
            'ه' => Some("i"),
            'خ' => Some("o"),
            'ح' => Some("p"),
            'ج' => Some("["),
            'د' => Some("]"),
            'ش' => Some("a"),
            'س' => Some("s"),
            'ي' => Some("d"),
            'ب' => Some("f"),
            'ل' => Some("g"),
            'ا' => Some("h"),
            'ت' => Some("j"),
            'ن' => Some("k"),
            'م' => Some("l"),
            'ك' => Some(";"),
            'ط' => Some("'"),
            'ئ' => Some("z"),
            'ء' => Some("x"),
            'ؤ' => Some("c"),
            'ر' => Some("v"),
            'ى' => Some("n"),
            'ة' => Some("m"),
            'و' => Some(","),
            'ز' => Some("."),
            'ظ' => Some("/"),
            'أ' => Some("H"),
            'إ' => Some("Y"),
            'آ' => Some("N"),
            '؛' => Some("P"),
            '،' => Some("K"),
            '؟' => Some("?"),
            'َ' => Some("Q"),
            'ً' => Some("W"),
            'ُ' => Some("E"),
            'ٌ' => Some("R"),
            'ِ' => Some("A"),
            'ٍ' => Some("S"),
            'ـ' => Some("J"),
            'ْ' => Some("X"),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn converts_english_layout_to_arabic() {
            assert_eq!(convert_auto("sghl").0, "سلام");
            assert_eq!(convert_auto("hgsghl ugd;l").0, "السلام عليكم");
            assert_eq!(convert_auto("Hpl]").0, "أحمد");
        }

        #[test]
        fn converts_arabic_layout_to_english() {
            assert_eq!(convert_auto("اثممخ").0, "hello");
            assert_eq!(convert_auto("لاغث").0, "bye");
        }

        #[test]
        fn caps_lock_text_uses_base_letters() {
            assert_eq!(convert_auto("SGHL").0, "سلام");
        }
    }
}
