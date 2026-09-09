use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[cfg(windows)]
use windows::{
    Win32::{
        Media::Audio::{
            DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
            IMMNotificationClient_Impl, MMDeviceEnumerator, eMultimedia, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
        UI::Shell::PropertiesSystem::PROPERTYKEY,
    },
    core::{PCWSTR, implement},
};

/// Core Audio notification bridge. It reports only default *render multimedia*
/// changes; communications-role changes are intentionally ignored.
pub struct DefaultOutputWatcher {
    changed: Arc<AtomicBool>,
    #[cfg(windows)]
    enumerator: IMMDeviceEnumerator,
    #[cfg(windows)]
    client: IMMNotificationClient,
    #[cfg(windows)]
    com_initialized: bool,
}

#[cfg(windows)]
#[implement(IMMNotificationClient)]
struct NotificationClient {
    changed: Arc<AtomicBool>,
}

#[cfg(windows)]
#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for NotificationClient {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eRender && role == eMultimedia {
            self.changed.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

impl DefaultOutputWatcher {
    #[cfg(windows)]
    pub fn new() -> Result<Self, String> {
        // The audio-owner worker is a dedicated thread. Initializing COM here
        // keeps every Core Audio object and callback on its owning thread.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| format!("Core Audio enumerator: {error}"))?
        };
        let changed = Arc::new(AtomicBool::new(false));
        let client: IMMNotificationClient = NotificationClient {
            changed: Arc::clone(&changed),
        }
        .into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
            .map_err(|error| format!("Core Audio notification registration: {error}"))?;
        Ok(Self {
            changed,
            enumerator,
            client,
            com_initialized: initialized,
        })
    }

    #[cfg(not(windows))]
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            changed: Arc::new(AtomicBool::new(false)),
        })
    }

    #[must_use]
    pub fn take_notification(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }

    #[cfg(windows)]
    pub fn current_endpoint_id(&self) -> Result<String, String> {
        let endpoint = unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|error| format!("Core Audio default multimedia endpoint: {error}"))?
        };
        let id = unsafe { endpoint.GetId() }
            .map_err(|error| format!("Core Audio endpoint id: {error}"))?;
        let text = unsafe { id.to_string() }
            .map_err(|error| format!("Core Audio endpoint id text: {error}"));
        unsafe { CoTaskMemFree(Some(id.0.cast())) };
        text
    }

    #[cfg(target_os = "macos")]
    pub fn current_endpoint_id(&self) -> Result<String, String> {
        use coreaudio_sys::{
            AudioObjectGetPropertyData, AudioObjectPropertyAddress,
            kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMaster,
            kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
        };
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster,
        };
        let mut device: u32 = 0;
        let mut size = std::mem::size_of_val(&device) as u32;
        // CoreAudio writes one AudioDeviceID into the exact-sized owned buffer.
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut device as *mut u32).cast(),
            )
        };
        if status != 0 || device == 0 {
            Err(format!("macOS default output unavailable: {status}"))
        } else {
            Ok(format!("coreaudio:{device}"))
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    pub fn current_endpoint_id(&self) -> Result<String, String> {
        crate::AudioEngine::default_output_device_name().map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
impl Drop for DefaultOutputWatcher {
    fn drop(&mut self) {
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.client)
        };
        if self.com_initialized {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_latch_is_edge_triggered() {
        let changed = AtomicBool::new(true);
        assert!(changed.swap(false, Ordering::AcqRel));
        assert!(!changed.swap(false, Ordering::AcqRel));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "explicit Windows host smoke: requires a configured multimedia render endpoint"]
    fn windows_default_multimedia_endpoint_and_cpal_default_are_queryable() {
        let watcher = DefaultOutputWatcher::new().expect("Core Audio watcher must register");
        let endpoint_id = watcher
            .current_endpoint_id()
            .expect("Windows multimedia endpoint must resolve");
        let device_name = crate::AudioEngine::default_output_device_name()
            .expect("CPAL must resolve the Windows default output");
        assert!(!endpoint_id.trim().is_empty());
        assert!(!device_name.trim().is_empty());
        eprintln!("Windows multimedia endpoint: {device_name} [{endpoint_id}]");
    }
}
