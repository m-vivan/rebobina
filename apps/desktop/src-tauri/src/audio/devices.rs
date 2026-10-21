//! Consulta direta ao CoreAudio para saber, de cada microfone, se ele é o
//! padrão do sistema e se algum programa (ex.: Teams, Meet, Zoom) está usando.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;

type AudioObjectId = u32;
type OsStatus = i32;
type CfStringRef = *const c_void;

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

const SYSTEM_OBJECT: AudioObjectId = 1;
const PROP_DEVICES: u32 = four_cc(b"dev#");
const PROP_DEFAULT_INPUT: u32 = four_cc(b"dIn ");
const PROP_STREAMS: u32 = four_cc(b"stm#");
const PROP_NAME: u32 = four_cc(b"lnam");
const PROP_RUNNING_SOMEWHERE: u32 = four_cc(b"gone");
const SCOPE_GLOBAL: u32 = four_cc(b"glob");
const SCOPE_INPUT: u32 = four_cc(b"inpt");
const ELEMENT_MAIN: u32 = 0;
const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        object: AudioObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
    ) -> OsStatus;
    fn AudioObjectGetPropertyData(
        object: AudioObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> OsStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(s: CfStringRef, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFRelease(cf: *const c_void);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceState {
    pub name: String,
    pub is_default: bool,
    /// Algum programa está com este microfone aberto agora (inclusive o Rebobina).
    pub in_use: bool,
}

fn address(selector: u32, scope: u32) -> PropertyAddress {
    PropertyAddress {
        selector,
        scope,
        element: ELEMENT_MAIN,
    }
}

fn data_size(object: AudioObjectId, addr: &PropertyAddress) -> Option<u32> {
    let mut size = 0u32;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(object, addr, 0, ptr::null(), &mut size) };
    (status == 0).then_some(size)
}

fn read_u32(object: AudioObjectId, selector: u32, scope: u32) -> Option<u32> {
    let addr = address(selector, scope);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &addr,
            0,
            ptr::null(),
            &mut size,
            (&mut value as *mut u32).cast(),
        )
    };
    (status == 0).then_some(value)
}

fn device_ids() -> Vec<AudioObjectId> {
    let addr = address(PROP_DEVICES, SCOPE_GLOBAL);
    let Some(mut size) = data_size(SYSTEM_OBJECT, &addr) else {
        return Vec::new();
    };
    let count = size as usize / std::mem::size_of::<AudioObjectId>();
    let mut ids = vec![0 as AudioObjectId; count];
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &addr,
            0,
            ptr::null(),
            &mut size,
            ids.as_mut_ptr().cast(),
        )
    };
    if status != 0 {
        return Vec::new();
    }
    ids.truncate(size as usize / std::mem::size_of::<AudioObjectId>());
    ids
}

fn has_input(id: AudioObjectId) -> bool {
    data_size(id, &address(PROP_STREAMS, SCOPE_INPUT)).is_some_and(|size| size > 0)
}

fn device_name(id: AudioObjectId) -> Option<String> {
    let addr = address(PROP_NAME, SCOPE_GLOBAL);
    let mut cf: CfStringRef = ptr::null();
    let mut size = std::mem::size_of::<CfStringRef>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            id,
            &addr,
            0,
            ptr::null(),
            &mut size,
            (&mut cf as *mut CfStringRef).cast(),
        )
    };
    if status != 0 || cf.is_null() {
        return None;
    }
    let mut buffer = [0 as c_char; 512];
    let ok = unsafe { CFStringGetCString(cf, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) };
    unsafe { CFRelease(cf) };
    if ok == 0 {
        return None;
    }
    let name = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    Some(name.to_string_lossy().into_owned())
}

/// Todos os microfones, com padrão do sistema e uso atual.
pub fn input_devices_state() -> Vec<InputDeviceState> {
    let default_id = read_u32(SYSTEM_OBJECT, PROP_DEFAULT_INPUT, SCOPE_GLOBAL);
    device_ids()
        .into_iter()
        .filter(|&id| has_input(id))
        .filter_map(|id| {
            let name = device_name(id)?;
            let in_use = read_u32(id, PROP_RUNNING_SOMEWHERE, SCOPE_GLOBAL).unwrap_or(0) != 0;
            Some(InputDeviceState {
                name,
                is_default: Some(id) == default_id,
                in_use,
            })
        })
        .collect()
}

/// Dispositivos virtuais que não são microfones de verdade.
pub fn is_virtual_input(name: &str) -> bool {
    let lower = name.to_lowercase();
    ["blackhole", "teams audio", "zoomaudio", "zoom audio", "aggregate", "agregado", "soundflower", "loopback"]
        .iter()
        .any(|v| lower.contains(v))
}

/// Microfone que a chamada provavelmente está usando: um microfone real aberto
/// por outro programa. `ignore` é o que o próprio Rebobina tem aberto.
pub fn call_input(devices: &[InputDeviceState], ignore: Option<&str>) -> Option<String> {
    let mut in_use = devices
        .iter()
        .filter(|d| d.in_use && !is_virtual_input(&d.name) && Some(d.name.as_str()) != ignore);
    let first = in_use.next()?;
    // Mais de um aberto ao mesmo tempo: prefere o padrão do sistema, se for um deles.
    match in_use.next() {
        None => Some(first.name.clone()),
        Some(_) => devices
            .iter()
            .find(|d| d.in_use && d.is_default && !is_virtual_input(&d.name))
            .map(|d| d.name.clone())
            .or_else(|| Some(first.name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, is_default: bool, in_use: bool) -> InputDeviceState {
        InputDeviceState {
            name: name.into(),
            is_default,
            in_use,
        }
    }

    #[test]
    fn reads_input_devices_from_core_audio() {
        let devices = input_devices_state();
        assert!(devices.iter().filter(|d| d.is_default).count() <= 1);
        assert!(devices.iter().all(|d| !d.name.is_empty()));
    }

    #[test]
    fn picks_the_microphone_used_by_the_call() {
        let devices = [
            dev("Microfone (MacBook Pro)", false, false),
            dev("AirPods Pro de Vivan", true, true),
            dev("Microsoft Teams Audio", false, true),
        ];
        assert_eq!(call_input(&devices, None).as_deref(), Some("AirPods Pro de Vivan"));
    }

    #[test]
    fn ignores_the_microphone_rebobina_itself_has_open() {
        let devices = [
            dev("Microfone (MacBook Pro)", false, true),
            dev("AirPods Pro de Vivan", true, false),
        ];
        assert_eq!(call_input(&devices, Some("Microfone (MacBook Pro)")), None);
    }

    #[test]
    fn recognizes_virtual_inputs() {
        for name in ["Microsoft Teams Audio", "BlackHole 2ch", "ZoomAudioDevice", "Dispositivo Agregado"] {
            assert!(is_virtual_input(name), "{name}");
        }
        for name in ["AirPods Pro de Vivan", "Microfone (MacBook Pro)", "Brio 500"] {
            assert!(!is_virtual_input(name), "{name}");
        }
    }

    #[test]
    fn prefers_default_when_several_are_in_use() {
        let devices = [
            dev("Brio 500", false, true),
            dev("AirPods Pro de Vivan", true, true),
        ];
        assert_eq!(call_input(&devices, None).as_deref(), Some("AirPods Pro de Vivan"));
    }
}
