#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod libnfc;

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use libnfc::NfcHandle;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

struct AppState {
    session: Mutex<Option<NfcHandle>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CardInfo {
    uid: String,
    atqa: String,
    sak: String,
    card_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SectorPreview {
    sector: u8,
    blocks: Vec<String>,
    ascii: Vec<String>,
}

#[derive(Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ReaderStatus {
    available: bool,
    count: usize,
    first_connstring: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadSectorRequest {
    sector: u8,
    key_hex: String,
    key_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteBlockRequest {
    sector: u8,
    block: u8,
    data_hex: String,
    key_hex: String,
    key_type: String,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DumpFile {
    format: String,
    uid: String,
    card_type: String,
    sectors: Vec<DumpSector>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DumpSector {
    sector: u8,
    blocks: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DumpToFileRequest {
    path: String,
    key_hex: String,
    key_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadDumpFileRequest {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteDumpToCardRequest {
    path: String,
    key_hex: String,
    key_type: String,
    write_trailer: bool,
}

#[tauri::command]
fn probe_reader_status() -> Result<ReaderStatus, String> {
    let (count, first_connstring) = NfcHandle::probe()?;
    Ok(ReaderStatus {
        available: count > 0,
        count,
        first_connstring,
    })
}

#[tauri::command]
fn connect_reader(state: State<AppState>) -> Result<String, String> {
    let mut lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    if let Some(existing) = lock.take() {
        drop(existing);
    }

    let session = NfcHandle::connect()?;
    let name = session.device_name()?;
    *lock = Some(session);
    Ok(name)
}

#[tauri::command]
fn disconnect_reader(state: State<AppState>) -> Result<(), String> {
    let mut lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    lock.take();
    Ok(())
}

#[tauri::command]
fn read_card_info(state: State<AppState>) -> Result<CardInfo, String> {
    let lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    let session = lock.as_ref().ok_or("reader not connected")?;
    let (uid, atqa, sak, card_type) = session.scan()?;
    Ok(CardInfo {
        uid,
        atqa,
        sak,
        card_type,
    })
}

#[tauri::command]
fn read_sector(state: State<AppState>, request: ReadSectorRequest) -> Result<SectorPreview, String> {
    if request.sector > 15 {
        return Err("only Mifare Classic 1K sectors (0-15) are supported".to_string());
    }

    let key = parse_fixed_hex::<6>(&request.key_hex, "keyHex")?;
    let key_type = parse_key_type(&request.key_type)?;

    let lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    let session = lock.as_ref().ok_or("reader not connected")?;

    let bytes = session.read_sector(request.sector, key, key_type)?;
    let mut blocks = Vec::with_capacity(4);
    let mut ascii = Vec::with_capacity(4);

    for chunk in bytes.chunks(16) {
        blocks.push(
            chunk
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" "),
        );
        ascii.push(
            chunk
                .iter()
                .map(|b| {
                    if (0x20..=0x7E).contains(b) {
                        *b as char
                    } else {
                        '.'
                    }
                })
                .collect(),
        );
    }

    Ok(SectorPreview {
        sector: request.sector,
        blocks,
        ascii,
    })
}

#[tauri::command]
fn write_block(state: State<AppState>, request: WriteBlockRequest) -> Result<(), String> {
    if request.sector > 15 {
        return Err("only Mifare Classic 1K sectors (0-15) are supported".to_string());
    }
    if request.block > 3 {
        return Err("block must be in range 0..3".to_string());
    }

    let key = parse_fixed_hex::<6>(&request.key_hex, "keyHex")?;
    let data = parse_fixed_hex::<16>(&request.data_hex, "dataHex")?;
    let key_type = parse_key_type(&request.key_type)?;

    let lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    let session = lock.as_ref().ok_or("reader not connected")?;
    session.write_block(request.sector, request.block, data, key, key_type)
}

#[tauri::command]
fn read_all_sectors_to_file(
    state: State<AppState>,
    request: DumpToFileRequest,
) -> Result<DumpFile, String> {
    let key = parse_fixed_hex::<6>(&request.key_hex, "keyHex")?;
    let key_type = parse_key_type(&request.key_type)?;

    let lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    let session = lock.as_ref().ok_or("reader not connected")?;
    let (uid, _, _, card_type) = session.scan()?;

    let mut sectors = Vec::with_capacity(16);
    for sector in 0..16u8 {
        let bytes = session.read_sector(sector, key, key_type)?;
        let blocks = bytes
            .chunks(16)
            .map(hex_bytes_no_space)
            .collect::<Vec<String>>();
        sectors.push(DumpSector { sector, blocks });
    }

    let dump = DumpFile {
        format: "mifare-classic-1k-v1".to_string(),
        uid,
        card_type,
        sectors,
    };
    save_dump(&request.path, &dump)?;
    Ok(dump)
}

#[tauri::command]
fn load_dump_file(request: LoadDumpFileRequest) -> Result<DumpFile, String> {
    load_dump(&request.path)
}

#[tauri::command]
fn write_dump_file_to_card(
    state: State<AppState>,
    request: WriteDumpToCardRequest,
) -> Result<(), String> {
    let key = parse_fixed_hex::<6>(&request.key_hex, "keyHex")?;
    let key_type = parse_key_type(&request.key_type)?;
    let dump = load_dump(&request.path)?;

    if dump.sectors.len() != 16 {
        return Err("dump file must contain exactly 16 sectors".to_string());
    }

    let lock = state.session.lock().map_err(|_| "state lock poisoned")?;
    let session = lock.as_ref().ok_or("reader not connected")?;
    let _ = session.scan()?;

    for sector in &dump.sectors {
        if sector.sector > 15 {
            return Err(format!("invalid sector in dump: {}", sector.sector));
        }
        if sector.blocks.len() != 4 {
            return Err(format!("sector {} must have 4 blocks", sector.sector));
        }
        let max_block = if request.write_trailer { 3 } else { 2 };
        for block in 0..=max_block {
            let data = parse_fixed_hex::<16>(&sector.blocks[block as usize], "dump block")?;
            session.write_block(sector.sector, block, data, key, key_type)?;
        }
    }
    Ok(())
}

fn parse_key_type(key_type: &str) -> Result<u8, String> {
    match key_type.to_ascii_uppercase().as_str() {
        "A" => Ok(0),
        "B" => Ok(1),
        _ => Err("keyType must be 'A' or 'B'".to_string()),
    }
}

fn parse_fixed_hex<const N: usize>(input: &str, field_name: &str) -> Result<[u8; N], String> {
    let compact: String = input.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if compact.len() != N * 2 {
        return Err(format!("{field_name} must be exactly {} hex chars", N * 2));
    }

    let mut out = [0u8; N];
    for i in 0..N {
        let start = i * 2;
        let end = start + 2;
        out[i] = u8::from_str_radix(&compact[start..end], 16)
            .map_err(|_| format!("{field_name} contains invalid hex"))?;
    }
    Ok(out)
}

fn hex_bytes_no_space(chunk: &[u8]) -> String {
    chunk
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join("")
}

fn save_dump(path: &str, dump: &DumpFile) -> Result<(), String> {
    let body = serde_json::to_string_pretty(dump).map_err(|e| format!("serialize dump failed: {e}"))?;
    fs::write(path, body).map_err(|e| format!("write dump failed: {e}"))
}

fn load_dump(path: &str) -> Result<DumpFile, String> {
    let body = fs::read_to_string(path).map_err(|e| format!("read dump failed: {e}"))?;
    serde_json::from_str::<DumpFile>(&body).map_err(|e| format!("invalid dump file: {e}"))
}

fn detect_reader_presence() -> ReaderStatus {
    let prefixes = [
        "tty.usbserial",
        "cu.usbserial",
        "tty.wchusbserial",
        "cu.wchusbserial",
        "tty.usbmodem",
        "cu.usbmodem",
    ];

    let mut ports = Vec::new();
    if let Ok(entries) = fs::read_dir("/dev") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if prefixes.iter().any(|p| name.starts_with(p)) {
                    ports.push(format!("/dev/{name}"));
                }
            }
        }
    }
    ports.sort_unstable();
    let first_connstring = ports
        .first()
        .map(|p| format!("pn532_uart:{p}"))
        .unwrap_or_default();
    ReaderStatus {
        available: !ports.is_empty(),
        count: ports.len(),
        first_connstring,
    }
}

fn should_handle_dev_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(_)
            | EventKind::Other
    )
}

fn start_reader_presence_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_secs(1)),
        ) {
            Ok(w) => w,
            Err(_) => return,
        };

        if watcher
            .watch(Path::new("/dev"), RecursiveMode::NonRecursive)
            .is_err()
        {
            return;
        }

        let mut last = detect_reader_presence();
        let _ = app.emit("reader-presence-changed", &last);

        loop {
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok(Ok(event)) => {
                    if should_handle_dev_event(&event.kind) {
                        let current = detect_reader_presence();
                        if current != last {
                            last = current.clone();
                            let _ = app.emit("reader-presence-changed", &current);
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Fallback: some systems may miss /dev removal notifications.
                    let current = detect_reader_presence();
                    if current != last {
                        last = current.clone();
                        let _ = app.emit("reader-presence-changed", &current);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            session: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            probe_reader_status,
            connect_reader,
            disconnect_reader,
            read_card_info,
            read_sector,
            write_block,
            read_all_sectors_to_file,
            load_dump_file,
            write_dump_file_to_card
        ])
        .setup(|app| {
            start_reader_presence_watcher(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
