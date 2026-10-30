//! The system share sheet for "Tell a friend".
//!
//! The WebView2 host has no `navigator.share`, and the WinRT class that shows
//! the sheet cannot be created the way a UWP app creates it. Desktop apps,
//! packaged or not, go through `IDataTransferManagerInterop`, which attaches
//! the sheet to one window; Microsoft documents it as the path for both.

/// Show the sheet the system uses, so the reader can pick WhatsApp, Mail or
/// whatever else is installed.
///
/// Async on purpose: a command without `async` runs on the main thread, which
/// is the thread that has to run the closure below, so waiting for the answer
/// there would wait for itself.
#[tauri::command]
pub async fn share_app(
    window: tauri::WebviewWindow,
    title: String,
    text: String,
    url: String,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        use windows::Win32::Foundation::HWND;

        let handle = window.hwnd().map_err(|error| error.to_string())?;
        // The handle travels as a number: a window handle is only meaningful on
        // the thread that owns the window, and the closure below runs there.
        let raw = handle.0 as isize;
        let (done, answer) = tokio::sync::oneshot::channel();
        // The sheet belongs to the window, and the window belongs to the main
        // thread, which is the only one that can show it.
        window
            .app_handle()
            .run_on_main_thread(move || {
                let _ = done.send(sheet::show(HWND(raw as _), &title, &text, &url));
            })
            .map_err(|error| error.to_string())?;
        answer
            .await
            .map_err(|_| "the share window did not answer".to_string())?
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, title, text, url);
        Err("the system share window is wired for Windows only".to_string())
    }
}

#[cfg(target_os = "windows")]
mod sheet {
    use std::cell::RefCell;
    use std::ffi::c_void;

    use windows::core::{factory, Interface, GUID, HRESULT, HSTRING};
    use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
    use windows::Foundation::{TypedEventHandler, Uri};
    use windows::Win32::Foundation::HWND;

    /// The interop interface, which the WinRT metadata does not describe.
    #[windows::core::interface("3A3DCD6C-3EAB-43DC-BCDE-45671CE800C8")]
    unsafe trait IDataTransferManagerInterop: windows::core::IUnknown {
        unsafe fn get_for_window(
            &self,
            appwindow: HWND,
            riid: *const GUID,
            result: *mut *mut c_void,
        ) -> HRESULT;
        unsafe fn show_share_ui_for_window(&self, appwindow: HWND) -> HRESULT;
    }

    /// `DataTransferManager`'s own identifier, handed to `GetForWindow`.
    const DTM_IID: GUID = GUID::from_u128(0xa5caee9b_8708_49d1_8d36_67d25a8da00c);

    // The sheet reads the package after this call returns, so the handler has
    // to outlive it. The main thread keeps the last one instead of leaking one
    // per share.
    thread_local! {
        static LIVE: RefCell<Option<(
            DataTransferManager,
            TypedEventHandler<DataTransferManager, DataRequestedEventArgs>,
        )>> = const { RefCell::new(None) };
    }

    pub fn show(hwnd: HWND, title: &str, text: &str, url: &str) -> Result<(), String> {
        let interop: IDataTransferManagerInterop =
            factory::<DataTransferManager, IDataTransferManagerInterop>()
                .map_err(describe)?;
        let mut raw: *mut c_void = std::ptr::null_mut();
        unsafe { interop.get_for_window(hwnd, &DTM_IID, &mut raw) }
            .ok()
            .map_err(describe)?;
        let manager = unsafe { DataTransferManager::from_raw(raw) };

        // Strings rather than WinRT objects: the handler outlives this call and
        // must not hold the apartment's objects open.
        let title = title.to_string();
        let text = text.to_string();
        let url = url.to_string();
        let handler = TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(
            move |_, args| {
                let data = args.ok()?.Request()?.Data()?;
                data.Properties()?.SetTitle(&HSTRING::from(title.as_str()))?;
                data.SetText(&HSTRING::from(text.as_str()))?;
                data.SetWebLink(&Uri::CreateUri(&HSTRING::from(url.as_str()))?)?;
                Ok(())
            },
        );

        manager.DataRequested(&handler).map_err(describe)?;
        unsafe { interop.show_share_ui_for_window(hwnd) }
            .ok()
            .map_err(describe)?;
        LIVE.with(|live| *live.borrow_mut() = Some((manager, handler)));
        Ok(())
    }

    fn describe(error: windows::core::Error) -> String {
        error.message().to_string()
    }
}
