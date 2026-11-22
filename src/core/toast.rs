//! Всплывающее HUD-уведомление (Toast) при подключении к игровому серверу Roblox.
//!
//! Отображает регион сервера (страна, город, флаг) и измеренный реальный пинг.
//! Работает поверх игры (включая полноэкранный режим без рамок) через легковесное
//! layered Win32 окно (WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT).
//! Не перехватывает клики мыши и фокус клавиатуры у клиента Roblox.

#![allow(dead_code)]

#[cfg(windows)]
pub use imp::{show_server_join, show_server_toast};

#[cfg(not(windows))]
pub use stub::*;

#[cfg(not(windows))]
mod stub {
    pub fn show_server_toast(_title: &str, _body: &str) {}
    pub fn show_server_join(_region: &str, _ping_ms: Option<u32>, _lang: crate::i18n::Lang) {}
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject,
        DrawTextW, EndPaint, RoundRect, SelectObject, SetBkMode, SetTextColor, SetWindowRgn,
        DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT, TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
        GetWindowLongPtrW, IsWindow, PeekMessageW, PostMessageW, RegisterClassExW,
        SetLayeredWindowAttributes, SetWindowLongPtrW, ShowWindow, SystemParametersInfoW,
        TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, LWA_ALPHA, MSG, PM_REMOVE, SM_CXSCREEN,
        SM_CYSCREEN, SPI_GETWORKAREA, SW_SHOWNOACTIVATE, WM_CLOSE, WM_CREATE, WM_DESTROY,
        WM_ERASEBKGND, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    const CLASS_NAME: &[u16] = &[
        b'B' as u16, b'S' as u16, b'T' as u16, b'o' as u16, b'a' as u16, b's' as u16,
        b't' as u16, 0,
    ];

    static ACTIVE_TOAST: Mutex<Option<HWND>> = Mutex::new(None);
    static REGISTER_ONCE: std::sync::Once = std::sync::Once::new();

    struct ToastData {
        title: Vec<u16>,
        body: Vec<u16>,
        width: i32,
        height: i32,
    }

    fn encode_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn is_current_toast(hwnd: HWND) -> bool {
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        if let Ok(lock) = ACTIVE_TOAST.lock() {
            *lock == Some(hwnd)
        } else {
            false
        }
    }

    unsafe fn pump_messages(hwnd: HWND) {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe extern "system" fn toast_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_CREATE => {
                let cs = &*(lparam as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut ps);
                if hdc != 0 {
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ToastData;
                    if !ptr.is_null() {
                        let data = &*ptr;
                        // Тёмная элегантная карточка #16171c (RGB 22, 23, 28)
                        let bg_brush = CreateSolidBrush(0x001C1716);
                        // Тонкая рамка #30333d (RGB 48, 51, 61)
                        let border_pen = CreatePen(0, 1, 0x003D3330);
                        let old_brush = SelectObject(hdc, bg_brush);
                        let old_pen = SelectObject(hdc, border_pen);

                        RoundRect(hdc, 0, 0, data.width, data.height, 16, 16);

                        SelectObject(hdc, old_pen);
                        SelectObject(hdc, old_brush);
                        DeleteObject(border_pen);
                        DeleteObject(bg_brush);

                        SetBkMode(hdc, TRANSPARENT as i32);

                        let font_face = encode_wide("Segoe UI");

                        // Заголовок (плашка уведомления)
                        let title_font = CreateFontW(
                            -13, 0, 0, 0, 600,
                            0, 0, 0, 1, 0, 0, 5, 0,
                            font_face.as_ptr(),
                        );
                        let old_font = SelectObject(hdc, title_font);
                        SetTextColor(hdc, 0x00A09A92); // Приглушённый серо-голубой RGB(146, 154, 160)

                        let mut title_rc = RECT {
                            left: 18,
                            top: 12,
                            right: data.width - 18,
                            bottom: 30,
                        };
                        DrawTextW(
                            hdc,
                            data.title.as_ptr(),
                            -1,
                            &mut title_rc,
                            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                        );

                        // Основная строка (Страна, город + пинг)
                        let body_font = CreateFontW(
                            -17, 0, 0, 0, 700,
                            0, 0, 0, 1, 0, 0, 5, 0,
                            font_face.as_ptr(),
                        );
                        SelectObject(hdc, body_font);
                        SetTextColor(hdc, 0x00FAF8F7); // Яркий белый RGB(247, 248, 250)

                        let mut body_rc = RECT {
                            left: 18,
                            top: 34,
                            right: data.width - 18,
                            bottom: 62,
                        };
                        DrawTextW(
                            hdc,
                            data.body.as_ptr(),
                            -1,
                            &mut body_rc,
                            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                        );

                        SelectObject(hdc, old_font);
                        DeleteObject(title_font);
                        DeleteObject(body_font);
                    }
                    EndPaint(hwnd, &ps);
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                let ptr = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut ToastData;
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    fn run_toast(title: &str, body: &str) {
        unsafe {
            let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

            REGISTER_ONCE.call_once(|| {
                let hinstance = GetModuleHandleW(std::ptr::null());
                let mut wc: WNDCLASSEXW = std::mem::zeroed();
                wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
                wc.lpfnWndProc = Some(toast_wnd_proc);
                wc.hInstance = hinstance;
                wc.lpszClassName = CLASS_NAME.as_ptr();
                wc.hbrBackground = CreateSolidBrush(0x001C1716);
                let _ = RegisterClassExW(&wc);
            });

            let width = 380;
            let height = 74;

            let mut rc = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            let (screen_w, screen_h) = if SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut rc as *mut _ as *mut _,
                0,
            ) != 0
                && rc.right > rc.left
                && rc.bottom > rc.top
            {
                (rc.right, rc.bottom)
            } else {
                (
                    GetSystemMetrics(SM_CXSCREEN),
                    GetSystemMetrics(SM_CYSCREEN),
                )
            };

            let x = screen_w - width - 24;
            let y = screen_h - height - 24;

            let data = Box::into_raw(Box::new(ToastData {
                title: encode_wide(title),
                body: encode_wide(body),
                width,
                height,
            }));

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_LAYERED
                    | WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT,
                CLASS_NAME.as_ptr(),
                encode_wide("ByteStrap Toast").as_ptr(),
                WS_POPUP,
                x,
                y,
                width,
                height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                data as *const _ as *const std::ffi::c_void,
            );

            if hwnd == 0 {
                drop(Box::from_raw(data));
                return;
            }

            let rgn = CreateRoundRectRgn(0, 0, width, height, 16, 16);
            if rgn != 0 {
                SetWindowRgn(hwnd, rgn, 1);
            }

            if let Ok(mut lock) = ACTIVE_TOAST.lock() {
                if let Some(old_hwnd) = lock.take() {
                    if IsWindow(old_hwnd) != 0 {
                        PostMessageW(old_hwnd, WM_CLOSE, 0, 0);
                    }
                }
                *lock = Some(hwnd);
            }

            ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            // Плавное появление (Fade in)
            for alpha in (0..=240).step_by(30) {
                if !is_current_toast(hwnd) {
                    break;
                }
                SetLayeredWindowAttributes(hwnd, 0, alpha as u8, LWA_ALPHA);
                pump_messages(hwnd);
                std::thread::sleep(Duration::from_millis(15));
            }

            // Удержание на экране (~4.2 сек)
            let start = std::time::Instant::now();
            while start.elapsed() < Duration::from_millis(4200) {
                if !is_current_toast(hwnd) {
                    break;
                }
                pump_messages(hwnd);
                std::thread::sleep(Duration::from_millis(50));
            }

            // Плавное исчезновение (Fade out)
            for alpha in (0..=240).step_by(30).rev() {
                if !is_current_toast(hwnd) {
                    break;
                }
                SetLayeredWindowAttributes(hwnd, 0, alpha as u8, LWA_ALPHA);
                pump_messages(hwnd);
                std::thread::sleep(Duration::from_millis(15));
            }

            if let Ok(mut lock) = ACTIVE_TOAST.lock() {
                if *lock == Some(hwnd) {
                    *lock = None;
                }
            }

            if IsWindow(hwnd) != 0 {
                DestroyWindow(hwnd);
            }
        }
    }

    /// Показать всплывающее уведомление с заголовком и телом.
    pub fn show_server_toast(title: &str, body: &str) {
        let title = title.to_string();
        let body = body.to_string();
        std::thread::Builder::new()
            .name("bytestrap-toast".into())
            .spawn(move || {
                run_toast(&title, &body);
            })
            .ok();
    }

    /// Вспомогательная функция для отображения региона сервера и пинга.
    pub fn show_server_join(region: &str, ping_ms: Option<u32>, lang: crate::i18n::Lang) {
        let title = match lang {
            crate::i18n::Lang::Ru => "🌐 ByteStrap  •  Подключено к серверу",
            _ => "🌐 ByteStrap  •  Connected to server",
        };
        let body = if let Some(p) = ping_ms {
            format!("{region}  •  {p} ms")
        } else {
            region.to_string()
        };
        show_server_toast(title, &body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_smoke() {
        show_server_join("🇩🇪 Германия, Франкфурт", Some(28), crate::i18n::Lang::Ru);
        show_server_join("🇺🇸 US, Miami", Some(110), crate::i18n::Lang::En);
    }
}
