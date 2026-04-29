# LayoutFixer

Small Windows background tool written in Rust for fixing text typed with the wrong keyboard layout.

Example:

```text
sghl -> سلام
اثممخ -> hello
```

The app works on selected text in any app. Select the wrong text, press your configured shortcut, and it replaces the selection with the corrected text.

## Features

- Converts English-layout text to Arabic-layout text.
- Converts Arabic-layout text back to English-layout text.
- Lets you choose the conversion hotkey on first run.
- Saves the chosen hotkey in a config file so you do not choose it every time.
- Runs in the background without opening a CMD window.
- Uses `Ctrl+Alt+F12` as the exit hotkey.
- No external Rust crates are required.

## Requirements

- Windows
- Rust toolchain

Check Rust:

```powershell
cargo --version
```

## Build

From this folder:

```powershell
cargo build --release
```

The executable will be created here:

```text
target\release\layout_fixer.exe
```

## Run

```powershell
.\target\release\layout_fixer.exe
```

On first run:

1. Press OK in the message box.
2. Press the shortcut you want to use, for example `Ctrl+Alt+T`.
3. Release the keys.
4. The app saves the shortcut and runs in the background.

After that, select text in any app and press your shortcut.

## Config File

The app creates this file next to the executable:

```text
target\release\layout_fixer.cfg
```

Example:

```ini
# LayoutFixer hotkey config
# Exit hotkey is always Ctrl+Alt+F12
display=Ctrl+Alt+T
modifiers=3
vk=84
```

To change the shortcut, delete `layout_fixer.cfg` and run the app again.

## Exit

Press:

```text
Ctrl+Alt+F12
```

On some laptops, if `F12` requires `Fn`, press:

```text
Ctrl+Alt+Fn+F12
```

## Notes

- The app uses the clipboard internally, like pressing `Ctrl+C` then `Ctrl+V`.
- Some apps may block copying selected text.
- `Fn` alone usually cannot be detected by Windows, so it cannot be used as a standalone hotkey.
