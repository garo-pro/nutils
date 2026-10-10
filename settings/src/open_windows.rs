//! The windows open right now, as WinMurderer's sweep sees them: for picking a
//! rule's target, and for warning when a rule would hit more than was meant.

use crate::config::MatchKind;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

pub struct OpenWindow {
    pub title: String,
    pub class: String,
    /// The program's file name, lowercased (e.g. `notepad.exe`).
    pub exe: Option<String>,
    /// Belongs to this settings editor.
    pub own: bool,
    /// The desktop or the taskbar.
    pub shell: bool,
    /// Visible to Windows but not on screen (e.g. a suspended UWP app).
    pub cloaked: bool,
    /// A tool window that isn't in the taskbar or Alt+Tab, such as a
    /// screen-reader highlighter or a shell flyout.
    pub tool: bool,
}

impl OpenWindow {
    /// What a rule matching by `kind` would compare.
    pub fn field(&self, kind: MatchKind) -> Option<String> {
        match kind {
            MatchKind::Title => Some(self.title.clone()),
            MatchKind::Class => Some(self.class.clone()),
            MatchKind::Exe => self.exe.clone(),
        }
    }

    /// How the window reads in a list, e.g. "Untitled - Notepad (notepad.exe)".
    pub fn label(&self) -> String {
        match &self.exe {
            Some(exe) => format!("{} ({exe})", self.title),
            None => self.title.clone(),
        }
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    out.push(hwnd);
    windows::core::BOOL(1)
}

/// Every visible top-level window, front to back: exactly what the sweep acts on.
pub fn all() -> Vec<OpenWindow> {
    let mut hwnds: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut _ as isize));
    }
    let me = unsafe { GetCurrentProcessId() };
    hwnds
        .into_iter()
        .filter(|&h| unsafe { IsWindowVisible(h).as_bool() })
        .map(|h| {
            let class = class_name(h);
            let pid = owner_pid(h);
            OpenWindow {
                title: title(h),
                shell: matches!(class.as_str(), "Progman" | "Shell_TrayWnd" | "WorkerW"),
                class,
                exe: exe_name(pid),
                own: pid == me,
                cloaked: is_cloaked(h),
                tool: is_tool_window(h),
            }
        })
        .collect()
}

/// The windows worth offering in the picker: on screen, titled, not tool
/// windows, and not ours or the shell's.
pub fn pickable() -> Vec<OpenWindow> {
    all()
        .into_iter()
        .filter(|w| !w.own && !w.shell && !w.cloaked && !w.tool && !w.title.trim().is_empty())
        .collect()
}

fn title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n as usize])
}

fn owner_pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn exe_name(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        res.ok()?;
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        let name = full.rsplit(['\\', '/']).next().unwrap_or(&full);
        Some(name.to_ascii_lowercase())
    }
}

fn is_tool_window(hwnd: HWND) -> bool {
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked: u32 = 0;
    unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
    }
}
