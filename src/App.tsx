import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CardInfo, DumpFile, SectorPreview } from "./types";

type SectorDump = {
  sector: number;
  blocks: string[];
  error?: string;
};

type OperationLog = {
  id: number;
  level: "info" | "error";
  message: string;
  at: string;
};


type ReaderStatus = { available: boolean; count: number; firstConnstring: string };

const COMMON_KEYS = ["FFFFFFFFFFFF", "A0A1A2A3A4A5", "D3F7D3F7D3F7", "000000000000"];
const SECTOR_OPTIONS = Array.from({ length: 16 }, (_, i) => String(i));
const BLOCK_OPTIONS = ["0", "1", "2", "3"];

function normalizeHex(input: string, bytes: number): string {
  const compact = input.replace(/[^0-9a-fA-F]/g, "").toUpperCase();
  return compact.slice(0, bytes * 2).padEnd(bytes * 2, "0");
}

function formatBlockHex(raw: string): string {
  const compact = raw.replace(/[^0-9a-fA-F]/g, "").toUpperCase();
  const out: string[] = [];
  for (let i = 0; i < compact.length; i += 2) {
    out.push(compact.slice(i, i + 2));
  }
  return out.join(" ");
}

export default function App() {
  const [connected, setConnected] = useState(false);
  const [deviceName, setDeviceName] = useState("");
  const [cardInfo, setCardInfo] = useState<CardInfo | null>(null);
  const [sectorData, setSectorData] = useState<SectorPreview | null>(null);
  const [allSectors, setAllSectors] = useState<SectorDump[]>(() => {
    return Array.from({ length: 16 }, (_, i) => ({
      sector: i,
      blocks: []
    }));
  });
  const [loadedDump, setLoadedDump] = useState<DumpFile | null>(null);


  const [sector, setSector] = useState("1");
  const [sectorFilter, setSectorFilter] = useState("");
  const [keyHex, setKeyHex] = useState("FFFFFFFFFFFF");
  const [keyType, setKeyType] = useState<"A" | "B">("A");

  const [writeSector, setWriteSector] = useState("1");
  const [writeBlock, setWriteBlock] = useState("0");
  const [writeData, setWriteData] = useState("00000000000000000000000000000000");

  const [dumpPath, setDumpPath] = useState("./card_dump.json");
  const [writeTrailer, setWriteTrailer] = useState(false);

  const [pendingOps, setPendingOps] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [logs, setLogs] = useState<OperationLog[]>([]);
  const [readerAvailable, setReaderAvailable] = useState(false);
  const [readerCount, setReaderCount] = useState(0);
  const [readerConnstring, setReaderConnstring] = useState("");
  const prevAvailableRef = useRef<boolean | null>(null);
  const queueTailRef = useRef<Promise<void>>(Promise.resolve());
  const autoReadingRef = useRef(false);
  const lastCardUidRef = useRef<string | null>(null);
  const noCardRef = useRef(false);
  const autoReadErrorRef = useRef<string | null>(null);
  const busy = pendingOps > 0;

  const connectLabel = useMemo(() => (connected ? "断开读卡器" : "连接读卡器"), [connected]);

  const filteredSectors = useMemo(() => {
    const q = sectorFilter.trim();
    if (!q) return allSectors;
    return allSectors.filter((s) => String(s.sector) === q);
  }, [allSectors, sectorFilter]);

  function addLog(level: "info" | "error", message: string) {
    setLogs((prev) => [{ id: Date.now(), level, message, at: new Date().toLocaleTimeString() }, ...prev].slice(0, 80));
  }

  function clearLogs() {
    setLogs([]);
  }

  useEffect(() => {
    const unlistenPromise = listen<ReaderStatus>("reader-presence-changed", (event) => {
      const status = event.payload;
      setReaderAvailable(status.available);
      setReaderCount(status.count);
      setReaderConnstring(status.firstConnstring);

      if (prevAvailableRef.current === null) {
        prevAvailableRef.current = status.available;
      } else if (prevAvailableRef.current !== status.available) {
        addLog("info", status.available ? "检测到NFC设备已插入" : "检测到NFC设备已移除");
        prevAvailableRef.current = status.available;
      }

      if (!status.available && connected) {
        setConnected(false);
        setDeviceName("");
        setCardInfo(null);
        lastCardUidRef.current = null;
        noCardRef.current = false;
        autoReadErrorRef.current = null;
      }
    });

    invoke<ReaderStatus>("probe_reader_status")
      .then((status) => {
        setReaderAvailable(status.available);
        setReaderCount(status.count);
        setReaderConnstring(status.firstConnstring);
        prevAvailableRef.current = status.available;
      })
      .catch((err) => {
        const message = err instanceof Error ? err.message : String(err);
        addLog("error", `设备探测失败: ${message}`);
      });

    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [connected]);

  function queuedInvoke<T>(name: string, payload?: Record<string, unknown>): Promise<T> {
    // 对于连接/断开操作，不使用队列，直接执行
    if (name === "connect_reader" || name === "disconnect_reader") {
      return invoke<T>(name, payload);
    }
    const task = queueTailRef.current.then(() => invoke<T>(name, payload));
    queueTailRef.current = task.then(
      () => undefined,
      () => undefined
    );
    return task;
  }

  async function call<T>(name: string, payload?: Record<string, unknown>): Promise<T> {
    setPendingOps((v) => v + 1);
    setError(null);
    try {
      return await queuedInvoke<T>(name, payload);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      addLog("error", `${name}: ${message}`);
      throw err;
    } finally {
      setPendingOps((v) => Math.max(0, v - 1));
    }
  }

  async function onConnect() {
    if (connected) {
      await call("disconnect_reader");
      setConnected(false);
      setCardInfo(null);
      setSectorData(null);
      setAllSectors(Array.from({ length: 16 }, (_, i) => ({
        sector: i,
        blocks: []
      })));
      setLoadedDump(null);
      setDeviceName("");
      addLog("info", "读卡器已断开");
      return;
    }

    const name = await call<string>("connect_reader");
    setConnected(true);
    setDeviceName(name);
    addLog("info", `读卡器连接成功: ${name}`);
  }

  async function onReadCard() {
    const info = await call<CardInfo>("read_card_info");
    setCardInfo(info);
    lastCardUidRef.current = info.uid;
    noCardRef.current = false;
    autoReadErrorRef.current = null;
    addLog("info", `读卡成功 UID=${info.uid}`);
  }

  async function tryAutoReadCard() {
    if (!connected || autoReadingRef.current) return;
    autoReadingRef.current = true;
    try {
      const info = await queuedInvoke<CardInfo>("read_card_info");
      setCardInfo((prev) => {
        if (
          prev &&
          prev.uid === info.uid &&
          prev.atqa === info.atqa &&
          prev.sak === info.sak &&
          prev.cardType === info.cardType
        ) {
          return prev;
        }
        return info;
      });

      if (lastCardUidRef.current !== info.uid) {
        addLog("info", `自动检测到卡片 UID=${info.uid}`);
      }
      lastCardUidRef.current = info.uid;
      noCardRef.current = false;
      autoReadErrorRef.current = null;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      const normalized = message.toLowerCase();
      const isNoCardError =
        normalized.includes("no card detected") ||
        normalized.includes("no card selected") ||
        normalized.includes("nfc_etimeout");

      if (isNoCardError) {
        if (!noCardRef.current && lastCardUidRef.current) {
          addLog("info", "卡片已移除");
        }
        setCardInfo(null);
        lastCardUidRef.current = null;
        noCardRef.current = true;
        autoReadErrorRef.current = null;
      } else if (autoReadErrorRef.current !== message) {
        autoReadErrorRef.current = message;
        addLog("error", `自动读取卡片失败: ${message}`);
      }
    } finally {
      autoReadingRef.current = false;
    }
  }

  useEffect(() => {
    if (!connected) {
      setCardInfo(null);
      lastCardUidRef.current = null;
      noCardRef.current = false;
      autoReadErrorRef.current = null;
      return;
    }

    let cancelled = false;
    let timer: number | null = null;
    const delayMs = 1500;

    const scheduleNext = () => {
      if (cancelled) return;
      timer = window.setTimeout(() => {
        void pollOnce();
      }, delayMs);
    };

    const pollOnce = async () => {
      if (cancelled) return;
      await tryAutoReadCard();
      scheduleNext();
    };

    void pollOnce();

    return () => {
      cancelled = true;
      if (timer !== null) {
        window.clearTimeout(timer);
      }
    };
  }, [connected]);

  function applyDumpToView(dump: DumpFile) {
    const rows: SectorDump[] = dump.sectors
      .slice()
      .sort((a, b) => a.sector - b.sector)
      .map((s) => ({ sector: s.sector, blocks: s.blocks }));
    setAllSectors(rows);
    setLoadedDump(dump);
  }

  async function readOneSector(sectorNumber: number): Promise<SectorPreview> {
    return call<SectorPreview>("read_sector", {
      request: {
        sector: sectorNumber,
        keyHex: normalizeHex(keyHex, 6),
        keyType
      }
    });
  }

  async function fetchSectorByNumber(sectorNumber: number) {
    setError(null);
    try {
      const result = await readOneSector(sectorNumber);
      setSectorData(result);
      setSector(String(sectorNumber));
      setAllSectors(prev => {
        const newSectors = [...prev];
        newSectors[sectorNumber] = {
          sector: sectorNumber,
          blocks: result.blocks
        };
        return newSectors;
      });
      addLog("info", `读取扇区 ${sectorNumber} 成功`);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setAllSectors(prev => {
        const newSectors = [...prev];
        newSectors[sectorNumber] = {
          sector: sectorNumber,
          blocks: [],
          error: message
        };
        return newSectors;
      });
      addLog("error", `读取扇区 ${sectorNumber} 失败: ${message}`);
      throw err;
    }
  }

  async function fetchSector() {
    await fetchSectorByNumber(Number(sector));
  }

  async function onReadAllSectors() {
    setError(null);
    const rows: SectorDump[] = [];
    for (let i = 0; i < 16; i++) {
      try {
        const result = await readOneSector(i);
        rows.push({ sector: i, blocks: result.blocks });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        rows.push({ sector: i, blocks: [], error: message });
      }
    }
    setAllSectors(rows);
    addLog("info", "读取16扇区完成");
  }

  async function onReadSector(e: FormEvent) {
    e.preventDefault();
    await fetchSector();
  }

  async function onReadSaveDump() {
    const dump = await call<DumpFile>("read_all_sectors_to_file", {
      request: {
        path: dumpPath,
        keyHex: normalizeHex(keyHex, 6),
        keyType
      }
    });
    applyDumpToView(dump);
    addLog("info", `已保存整卡数据: ${dumpPath}`);
  }

  async function onLoadDumpFile() {
    const dump = await call<DumpFile>("load_dump_file", {
      request: { path: dumpPath }
    });
    applyDumpToView(dump);
    addLog("info", `已加载文件: ${dumpPath}`);
  }

  async function onWriteDumpFile() {
    await call("write_dump_file_to_card", {
      request: {
        path: dumpPath,
        keyHex: normalizeHex(keyHex, 6),
        keyType,
        writeTrailer
      }
    });
    addLog("info", `已写入卡片: ${dumpPath}${writeTrailer ? " (含B3)" : ""}`);
    await onReadAllSectors();
  }

  async function onWriteBlock(e: FormEvent) {
    e.preventDefault();
    await call("write_block", {
      request: {
        sector: Number(writeSector),
        block: Number(writeBlock),
        dataHex: normalizeHex(writeData, 16),
        keyHex: normalizeHex(keyHex, 6),
        keyType
      }
    });
    addLog("info", `写入扇区${writeSector} 块${writeBlock} 成功`);
    await fetchSectorByNumber(Number(writeSector));
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand card">
          <h1>Mac NFC Tool</h1>
          <p>PN532 + Mifare Classic</p>
        </div>

        <section className="card">
          <h2>设备信息</h2>
          <div className="kv-list">
            <div>
              <span>读卡器</span>
              <strong>{deviceName || "未连接"}</strong>
            </div>
            <div>
              <span>设备检测</span>
              <strong>
                <span className={`led ${readerAvailable ? "online" : "offline"}`} />
                {readerAvailable ? "已检测到" : "未检测到"}
              </strong>
            </div>
            <div>
              <span>连接状态</span>
              <strong className={connected ? "ok" : "muted"}>{connected ? "已连接" : "未连接"}</strong>
            </div>
            <div>
              <span>设备数量</span>
              <strong>{readerCount}</strong>
            </div>
            <div>
              <span>连接串</span>
              <strong>{readerConnstring || "-"}</strong>
            </div>
          </div>
        </section>

        <section className="card">
          <h2>卡片信息</h2>
          <div className="kv-list">
            <div>
              <span>卡UID</span>
              <strong>{cardInfo?.uid || "-"}</strong>
            </div>
            <div>
              <span>ATQA</span>
              <strong>{cardInfo?.atqa || "-"}</strong>
            </div>
            <div>
              <span>SAK</span>
              <strong>{cardInfo?.sak || "-"}</strong>
            </div>
            <div>
              <span>类型</span>
              <strong>{cardInfo?.cardType || "-"}</strong>
            </div>
          </div>
        </section>

        <section className="card">
          <div className="card-header">
            <h2>操作按钮</h2>
            <span className={`status-pill ${busy ? "busy" : connected ? "ok" : "muted"}`}>{busy ? "处理中" : connected ? "已连接" : "未连接"}</span>
          </div>
          <div className="action-buttons">
            <button onClick={onConnect} disabled={busy}>{connectLabel}</button>
            <button onClick={onReadCard} disabled={!connected || busy}>读取卡片信息</button>
          </div>
        </section>

        <section className="card">
          <h2>密钥预设</h2>
          <div className="preset-grid">
            {COMMON_KEYS.map((k, index) => (
              <label key={k} className="preset-option">
                <input 
                  type="radio" 
                  name="keyPreset" 
                  checked={keyHex === k} 
                  onChange={() => setKeyHex(k)}
                />
                <span>{k}</span>
              </label>
            ))}
          </div>
          <div className="key-type-selector">
            <label>
              密钥类型
              <select value={keyType} onChange={(e) => setKeyType(e.target.value as "A" | "B")}>
                <option value="A">Key A</option>
                <option value="B">Key B</option>
              </select>
            </label>
          </div>
        </section>

        <section className="card">
          <h2>数据文件</h2>
          <div className="sidebar-form">
            <label>
              文件路径(JSON)
              <input value={dumpPath} onChange={(e) => setDumpPath(e.target.value)} />
            </label>
            <button type="button" onClick={onReadSaveDump} disabled={!connected || busy}>读取并保存16扇区</button>
            <button type="button" onClick={onLoadDumpFile} disabled={busy}>从文件加载预览</button>
            <label className="inline-check">
              <input type="checkbox" checked={writeTrailer} onChange={(e) => setWriteTrailer(e.target.checked)} />
              写入 trailer block(B3，风险高)
            </label>
            <button type="button" onClick={onWriteDumpFile} disabled={!connected || busy}>将文件写入卡片</button>
          </div>
        </section>

      </aside>

      <section className="content-area">


        <section className="card">
          <div className="section-head">
            <h2>16扇区 HEX 总览</h2>
            <div className="overview-actions">
              <div className="sector-buttons">
                <button type="button" className="mini" onClick={() => setSectorFilter("")} disabled={!connected || busy}>
                  全部
                </button>
                {SECTOR_OPTIONS.slice(0, 8).map((n) => (
                  <button key={n} type="button" className="mini" onClick={() => setSectorFilter(n)} disabled={!connected || busy}>
                    {n}
                  </button>
                ))}
              </div>
              <div className="sector-buttons">
                {SECTOR_OPTIONS.slice(8).map((n) => (
                  <button key={n} type="button" className="mini" onClick={() => setSectorFilter(n)} disabled={!connected || busy}>
                    {n}
                  </button>
                ))}
                <button className="mini action-button read" onClick={onReadAllSectors} disabled={!connected || busy} title="读取全部数据">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"></path>
                    <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"></path>
                  </svg>
                </button>
              </div>
            </div>
          </div>
          <div className="sector-grid">
            {filteredSectors.map((entry) => (
              <div className="sector-card" key={entry.sector}>
                <div className="sector-head">
                  <h3>Sector {entry.sector}</h3>
                  <div className="sector-actions">
                    <button className="mini action-button read" onClick={() => fetchSectorByNumber(entry.sector)} disabled={!connected || busy} title="读取">
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <circle cx="12" cy="12" r="10"></circle>
                        <line x1="12" y1="8" x2="12" y2="12"></line>
                        <line x1="12" y1="16" x2="12.01" y2="16"></line>
                      </svg>
                    </button>
                    <button className="mini action-button write" onClick={() => {
                      setWriteSector(String(entry.sector));
                      // 不再切换视图，保持在总览页面
                    }} disabled={!connected || busy} title="写入">
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
                        <polyline points="14 2 14 8 20 8"></polyline>
                        <line x1="16" y1="13" x2="8" y2="13"></line>
                        <line x1="16" y1="17" x2="8" y2="17"></line>
                        <polyline points="10 9 9 9 8 9"></polyline>
                      </svg>
                    </button>
                  </div>
                </div>
                {entry.error ? (
                  <div className="sector-error">{entry.error}</div>
                ) : (
                  entry.blocks.map((blockHex, idx) => (
                    <div className="hex-line" key={idx}>
                      <span className="idx">B{idx}</span>
                      <code>{formatBlockHex(blockHex)}</code>
                    </div>
                  ))
                )}
              </div>
            ))}
          </div>
        </section>

      </section>

      <aside className="rightbar">
        <section className="card log-card">
          <div className="section-head">
            <h2>操作日志</h2>
            <div className="log-head-actions">
              <span className="muted">{logs.length} 条</span>
              <button type="button" className="mini" onClick={clearLogs} disabled={logs.length === 0}>
                清空
              </button>
            </div>
          </div>
          <div className="log-list">
            {logs.length === 0 ? <p className="muted">暂无日志</p> : null}
            {logs.map((log) => (
              <div key={log.id} className={`log-item ${log.level}`}>
                <span>{log.at}</span>
                <span>{log.message}</span>
              </div>
            ))}
          </div>
        </section>
      </aside>
    </main>
  );
}
