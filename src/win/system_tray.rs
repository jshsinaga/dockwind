use anyhow::{Result, ensure};
use tracing::debug;
use windows::{
    Win32::{
        Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME},
        Media::Audio::{
            EDataFlow, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
            eCapture, eMultimedia, eRender,
        },
        Networking::NetworkListManager::{
            INetworkListManager, NLM_CONNECTIVITY_DISCONNECTED, NLM_CONNECTIVITY_IPV4_INTERNET,
            NLM_CONNECTIVITY_IPV6_INTERNET, NetworkListManager,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        },
        UI::{
            Input::KeyboardAndMouse::GetKeyboardLayout,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
    core::GUID,
};

use super::dialogs::ComApartmentGuard;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioAction {
    SetOutputMute(bool),
    SetMicrophoneMute(bool),
    ChangeVolume(f32),
}

pub struct SoundStatus {
    pub percentage: u8,
    pub muted: bool,
}

#[derive(Default)]
pub struct TrayStatus {
    pub network: Option<&'static str>,
    pub sound: Option<SoundStatus>,
    pub microphone_muted: Option<bool>,
    pub power: Option<String>,
    pub input_language: Option<String>,
}

pub fn status() -> TrayStatus {
    let _com = ComApartmentGuard::new();
    TrayStatus {
        network: read(network_status()),
        sound: read(sound_status()),
        microphone_muted: read(mute_status(eCapture)),
        power: read(power_status()).flatten(),
        input_language: input_language(),
    }
}

pub fn apply_audio(action: AudioAction) -> Result<()> {
    let _com = ComApartmentGuard::new();
    let flow = if matches!(action, AudioAction::SetMicrophoneMute(_)) {
        eCapture
    } else {
        eRender
    };
    let endpoint = audio_endpoint(flow)?;
    let context = GUID::zeroed();
    // SAFETY: the endpoint was acquired on this COM thread and context lives
    // through the call. The requested volume is a valid normalized scalar.
    unsafe {
        match action {
            AudioAction::SetOutputMute(muted) | AudioAction::SetMicrophoneMute(muted) => {
                endpoint.SetMute(muted, &context)?;
            }
            AudioAction::ChangeVolume(delta) => {
                let level = endpoint.GetMasterVolumeLevelScalar()?;
                endpoint.SetMasterVolumeLevelScalar((level + delta).clamp(0.0, 1.0), &context)?;
            }
        }
    }
    Ok(())
}

fn read<T>(result: Result<T>) -> Option<T> {
    result
        .inspect_err(|error| debug!(%error, "system tray status unavailable"))
        .ok()
}

fn audio_endpoint(flow: EDataFlow) -> Result<IAudioEndpointVolume> {
    // SAFETY: the caller initialized COM on this thread. Each interface stays
    // on this thread and Activate requests the typed audio endpoint interface.
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(flow, eMultimedia)?;
        Ok(device.Activate(CLSCTX_ALL, None)?)
    }
}

fn sound_status() -> Result<SoundStatus> {
    let endpoint = audio_endpoint(eRender)?;
    // SAFETY: these getters use the live endpoint on its owning COM thread.
    let (level, muted) = unsafe {
        (
            endpoint.GetMasterVolumeLevelScalar()?,
            endpoint.GetMute()?.as_bool(),
        )
    };
    ensure!(
        level.is_finite(),
        "audio endpoint returned an invalid volume"
    );
    Ok(SoundStatus {
        percentage: (level.clamp(0.0, 1.0) * 100.0).round() as u8,
        muted,
    })
}

fn mute_status(flow: EDataFlow) -> Result<bool> {
    let endpoint = audio_endpoint(flow)?;
    // SAFETY: the getter uses the live endpoint on its owning COM thread.
    Ok(unsafe { endpoint.GetMute()? }.as_bool())
}

fn network_status() -> Result<&'static str> {
    // SAFETY: the caller initialized COM, and the interface is used and
    // released on this thread.
    let connectivity = unsafe {
        let manager: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL)?;
        manager.GetConnectivity()?
    };
    Ok(if connectivity == NLM_CONNECTIVITY_DISCONNECTED {
        "Offline"
    } else if connectivity.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0)
        != 0
    {
        "Online"
    } else {
        "Local connection"
    })
}

fn power_status() -> Result<Option<String>> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: the power API writes to this initialized, writable structure.
    unsafe { GetSystemPowerStatus(&mut status)? };
    let percentage = status.BatteryLifePercent;
    if percentage > 100 || status.BatteryFlag == 255 || status.BatteryFlag & 128 != 0 {
        return Ok(None);
    }
    let source = if status.BatteryFlag & 8 != 0 {
        " charging"
    } else if status.ACLineStatus == 1 {
        " plugged in"
    } else {
        ""
    };
    Ok(Some(format!("{percentage}%{source}")))
}

fn input_language() -> Option<String> {
    // SAFETY: these APIs query the active window's thread and keyboard layout.
    // A zero thread id selects the current thread if there is no foreground.
    let layout =
        unsafe { GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None)) };
    let language = layout.0 as usize as u16;
    let mut code = [0; 12];
    // SAFETY: the locale takes a language id with the default sort value and
    // Windows writes at most the length of code.
    let length =
        unsafe { GetLocaleInfoW(language as u32, LOCALE_SISO639LANGNAME, Some(&mut code)) };
    (length > 1)
        .then(|| String::from_utf16_lossy(&code[..length as usize - 1]).to_ascii_uppercase())
}
