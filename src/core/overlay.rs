//! Оверлей «Картинка клиента» (Windows): окно-лупа (Magnification API)
//! поверх окна Roblox с матрицей цвета — яркость, контраст, насыщенность.
//! Клик-сквозь (WS_EX_TRANSPARENT на топ-левеле), оверлей исключён из
//! источника лупы через MagSetWindowFilterList (иначе рекурсия «зеркал»).
//! Fallback без Magnification — полупрозрачная чёрная/белая плёнка
//! (только яркость).

#![allow(dead_code)]

#[cfg(windows)]
pub use imp::{spawn, Handle, Params};

#[cfg(not(windows))]
pub use stub::*;

#[cfg(windows)]
mod imp {
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::CreateSolidBrush;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::Magnification::{
        MagInitialize, MagSetColorEffect, MagSetWindowFilterList, MagSetWindowSource,
        MagSetWindowTransform, MagUninitialize, MAGCOLOREFFECT, MAGTRANSFORM,
        MW_FILTERMODE_EXCLUDE, WC_MAGNIFIER,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
        GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PeekMessageW,
        RegisterClassExW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
        TranslateMessage, MSG, WNDCLASSEXW, WNDENUMPROC, HWND_TOPMOST, LWA_ALPHA,
        PM_REMOVE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNOACTIVATE,
        WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
    };

    const GAME_EXE: &str = "robloxplayerbeta.exe";
    const CLASS_NAME: &[u16] = &[
        b'B' as u16, b'S' as u16, b'O' as u16, b'v' as u16, b'e' as u16, b'r' as u16,
        b'l' as u16, b'a' as u16, b'y' as u16, 0,
    ];

    /// Параметры «картинки»: тумблер + три слайдера.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Params {
        pub enabled: bool,
        /// Яркость: -50..50 (0 = норма).
        pub bright: i32,
        /// Насыщенность: 0..200 (100 = норма).
        pub satur: i32,
        /// Контраст: 50..150 (100 = норма).
        pub contrast: i32,
    }

    impl Params {
        /// Матрица цвета 5×5 (GDI+ ColorMatrix): яркость → контраст →
        /// насыщенность. Строка 4 — аддитивные сдвиги.
        fn matrix(self) -> [f32; 25] {
            let b = (self.bright.clamp(-50, 50) as f32) / 100.0 + 1.0;
            let c = (self.contrast.clamp(50, 150) as f32) / 100.0;
            let s = (self.satur.clamp(0, 200) as f32) / 100.0;
            let lum = [0.3086f32, 0.6094, 0.0820];

            // База — единичная.
            let mut m = [0.0f32; 25];
            for i in 0..5 {
                m[i * 5 + i] = 1.0;
            }
            // Яркость: диагональ 0..2.
            for i in 0..3 {
                m[i * 5 + i] *= b;
            }
            // Контраст: сжатие к середине; сдвиг — в столбце 4 (GDI+).
            for i in 0..3 {
                m[i * 5 + i] *= c;
                m[i * 5 + 4] = 0.5 * (1.0 - c);
            }
            // Насыщенность: строки 0..2 = сатурация × (яркость·контраст).
            let bc = b * c;
            let mut r = [0.0f32; 25];
            for i in 0..3 {
                for j in 0..3 {
                    r[i * 5 + j] = (if i == j {
                        lum[i] * (1.0 - s) + s
                    } else {
                        lum[j] * (1.0 - s)
                    }) * bc;
                }
                r[i * 5 + 3] = m[i * 5 + 3];
                r[i * 5 + 4] = m[i * 5 + 4];
            }
            // Строки альфа и якорь — из единичной.
            for j in 0..5 {
                r[3 * 5 + j] = m[3 * 5 + j];
                r[4 * 5 + j] = m[4 * 5 + j];
            }
            r
        }
    }

    enum Msg {
        Params(Params),
        Stop,
    }

    /// Ручка для обновления параметров из контроллера.
    pub struct Handle {
        tx: Option<Sender<Msg>>,
    }

    impl Handle {
        pub fn update(&self, p: Params) {
            if let Some(tx) = &self.tx {
                let _ = tx.send(Msg::Params(p));
            }
        }

        pub fn stop(&mut self) {
            self.tx = None; // канал умирает → поток завершается
        }
    }

    /// Запустить поток оверлея.
    pub fn spawn(initial: Params) -> Handle {
        let (tx, rx) = channel::<Msg>();
        std::thread::Builder::new()
            .name("overlay".into())
            .spawn(move || run(rx, initial))
            .ok();
        Handle { tx: Some(tx) }
    }

    fn run(rx: Receiver<Msg>, initial: Params) {
        // Оверлей — per-monitor DPI: GetWindowRect вернёт физические пиксели.
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let mag_ok = unsafe { MagInitialize() } != 0;
        if !mag_ok {
            crate::core::logger::warn("Overlay: Magnification API недоступен — плёнка");
        }
        let mut params = initial;
        let mut dirty = true;

        let mut host: HWND = std::ptr::null_mut();
        let mut child: HWND = std::ptr::null_mut();
        let mut last_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut last_tick = Instant::now();

        loop {
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(Msg::Params(p)) => {
                    if params != p {
                        dirty = true;
                    }
                    params = p;
                }
                Ok(Msg::Stop) | Err(_) => break,
            }

            // Прокачка сообщений (перерисовка лупы и пр.).
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            unsafe {
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if last_tick.elapsed() < Duration::from_millis(33) {
                continue;
            }
            last_tick = Instant::now();

            let game = find_game_hwnd();
            if game.is_null() || unsafe { IsIconic(game) } != 0 || !params.enabled {
                if !host.is_null() && unsafe { IsWindowVisible(host) } != 0 {
                    unsafe { ShowWindow(host, SW_HIDE) };
                }
                continue;
            }

            let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if unsafe { GetWindowRect(game, &mut rc) } == 0
                || rc.right <= rc.left
                || rc.bottom <= rc.top
            {
                continue;
            }

            if host.is_null() {
                let (h, c) = create_windows(mag_ok, rc);
                if h.is_null() {
                    continue;
                }
                host = h;
                child = c;
                if mag_ok && !child.is_null() {
                    // Исключаем себя из источника — без этого рекурсия.
                    let mut mine = [host, child];
                    unsafe {
                        MagSetWindowFilterList(
                            child,
                            MW_FILTERMODE_EXCLUDE,
                            2,
                            mine.as_mut_ptr(),
                        );
                        let mut t = MAGTRANSFORM { v: [0.0; 9] };
                        t.v[0] = 1.0;
                        t.v[4] = 1.0;
                        t.v[8] = 1.0;
                        MagSetWindowTransform(child, &mut t);
                    }
                }
            }

            let moved = last_rect.left != rc.left
                || last_rect.top != rc.top
                || last_rect.right != rc.right
                || last_rect.bottom != rc.bottom;
            if moved {
                last_rect = rc;
                let w = rc.right - rc.left;
                let h = rc.bottom - rc.top;
                unsafe {
                    SetWindowPos(
                        host,
                        HWND_TOPMOST,
                        rc.left,
                        rc.top,
                        w,
                        h,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    if !child.is_null() {
                        SetWindowPos(
                            child,
                            std::ptr::null_mut(),
                            0,
                            0,
                            w,
                            h,
                            SWP_NOACTIVATE,
                        );
                    }
                }
            }

            if mag_ok && !child.is_null() {
                unsafe {
                    MagSetWindowSource(child, rc);
                    if dirty {
                        let mut m = MAGCOLOREFFECT { transform: params.matrix() };
                        MagSetColorEffect(child, &mut m);
                        dirty = false;
                    }
                }
            } else if !mag_ok {
                // Плёнка: только затемнение (осветлить плёнкой нельзя).
                if params.bright < 0 {
                    let a = ((params.bright.abs().min(50) as u32 * 255) / 100) as u8;
                    unsafe {
                        SetLayeredWindowAttributes(host, 0, a.max(1), LWA_ALPHA);
                        ShowWindow(host, SW_SHOWNOACTIVATE);
                    }
                } else if unsafe { IsWindowVisible(host) } != 0 {
                    unsafe { ShowWindow(host, SW_HIDE) };
                }
            }
        }

        // Очистка: окно создано в этом же потоке.
        if !host.is_null() {
            unsafe { DestroyWindow(host) };
        }
        if mag_ok {
            unsafe { MagUninitialize() };
        }
    }

    fn create_windows(mag_ok: bool, rc: RECT) -> (HWND, HWND) {
        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSEXW = std::mem::zeroed();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.hInstance = hinstance;
            wc.lpszClassName = CLASS_NAME.as_ptr();
            // Плёнка: чёрный фон (для +bright цвет не критичен — alpha-блендинг).
            wc.hbrBackground = CreateSolidBrush(0x000000);
            let _ = RegisterClassExW(&wc); // 0 = уже зарегистрирован — ок

            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;
            let ex = if mag_ok {
                WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
            } else {
                WS_EX_TOPMOST
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_LAYERED
            };
            let host = CreateWindowExW(
                ex,
                CLASS_NAME.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                rc.left,
                rc.top,
                w,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null(),
            );
            if host.is_null() {
                return (std::ptr::null_mut(), std::ptr::null_mut());
            }

            let child = if mag_ok {
                CreateWindowExW(
                    0,
                    WC_MAGNIFIER,
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    0,
                    w,
                    h,
                    host,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                )
            } else {
                std::ptr::null_mut()
            };
            ShowWindow(host, SW_SHOWNOACTIVATE);
            (host, child)
        }
    }

    /// Окно Roblox: видимое топ-левел окно процесса RobloxPlayerBeta.
    fn find_game_hwnd() -> HWND {
        extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> i32 {
            unsafe {
                let out = lparam as *mut HWND;
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == 0 {
                    return 1;
                }
                let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if proc.is_null() {
                    return 1;
                }
                let mut buf = [0u16; 260];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(proc, 0, buf.as_mut_ptr(), &mut len);
                CloseHandle(proc);
                if ok != 0 {
                    let path = String::from_utf16_lossy(&buf[..len as usize]);
                    let name = path
                        .rsplit(['\\', '/'])
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if name == GAME_EXE {
                        *out = hwnd;
                        return 0;
                    }
                }
                1
            }
        }

        let mut found: HWND = std::ptr::null_mut();
        unsafe {
            EnumWindows(Some(cb), &mut found as *mut HWND as LPARAM);
        }
        found
    }
    #[cfg(test)]
    mod tests {
        use super::Params;

        #[test]
        fn matrix_identity() {
            let m = Params { enabled: true, bright: 0, satur: 100, contrast: 100 }.matrix();
            let id = |i: usize, j: usize| if i == j { 1.0 } else { 0.0 };
            for i in 0..5 {
                for j in 0..5 {
                    assert!((m[i * 5 + j] - id(i, j)).abs() < 1e-4, "({i},{j})={}", m[i * 5 + j]);
                }
            }
        }

        #[test]
        fn matrix_bright() {
            let m = Params { enabled: true, bright: 50, satur: 100, contrast: 100 }.matrix();
            assert!((m[0] - 1.5).abs() < 1e-4);
            assert!((m[24] - 1.0).abs() < 1e-4);
            // Тёмная сторона.
            let d = Params { enabled: true, bright: -50, satur: 100, contrast: 100 }.matrix();
            assert!((d[0] - 0.5).abs() < 1e-4);
        }

        #[test]
        fn matrix_grayscale_and_contrast() {
            // Насыщенность 0 → оттенки серого (веса яркости по диагонали разбиты).
            let g = Params { enabled: true, bright: 0, satur: 0, contrast: 100 }.matrix();
            let lum = [0.3086f32, 0.6094, 0.0820];
            for i in 0..3 {
                for j in 0..3 {
                    assert!((g[i * 5 + j] - lum[j]).abs() < 1e-4, "({i},{j})");
                }
            }
            // Контраст 150 → диагональ 1.5, сдвиг (столбец 4) -0.25.
            let c = Params { enabled: true, bright: 0, satur: 100, contrast: 150 }.matrix();
            assert!((c[0] - 1.5).abs() < 1e-4);
            assert!((c[4] - -0.25).abs() < 1e-4);
            // Сдвиг не должен масштабироваться яркостью повторно.
            let bc = Params { enabled: true, bright: -50, satur: 100, contrast: 150 }.matrix();
            assert!((bc[0] - 0.75).abs() < 1e-4);
            assert!((bc[4] - -0.25).abs() < 1e-4);
        }
    }
}


#[cfg(not(windows))]
mod stub {
    //! Заглушка: оверлей доступен только на Windows.

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Params {
        pub enabled: bool,
        pub bright: i32,
        pub satur: i32,
        pub contrast: i32,
    }

    pub struct Handle;

    impl Handle {
        pub fn update(&self, _p: Params) {}
        pub fn stop(&mut self) {}
    }

    pub fn spawn(_initial: Params) -> Handle {
        Handle
    }
}
