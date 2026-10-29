# 00. System overview

Covers: FR-CON-*, NFR-MNT-1, NFR-MNT-5, NFR-REL-3. Decisions: ADR-0004, 0005, 0007, 0008.

## Components
```mermaid
flowchart TB
  subgraph Desktop
    direction TB
    UI["continuum-ui<br/>(GTK4 app)"]
    NAU["Nautilus extension<br/>(Python)"]
    SHELL["Shell extension<br/>(JS)"]
    subgraph D["continuumd (daemon)"]
      DBUS[D-Bus API] --> CORE[Device manager]
      CORE --> DISC[Discovery<br/>mDNS]
      CORE --> SESS[Sessions<br/>TLS control conns]
      CORE --> PLUG[Plugins<br/>share · clip · notif · call · …]
      PLUG --> DATA[Data channels<br/>files, video]
      CORE --> STORE[(Identity + paired devices<br/>~/.local/share/continuum)]
    end
    PROTO[[continuum-proto<br/>types · framing · crypto · pairing]]
    UI & NAU & SHELL -- session bus --> DBUS
    D -.uses.-> PROTO
  end
  subgraph Android
    APP["Compose UI"] --> SVC["ContinuumService<br/>(foreground)"]
    SVC --> APLUG[Plugins]
    SVC --> KS[(Keystore +<br/>DataStore)]
  end
  SESS <== "control: TLS 1.3, pinned" ==> SVC
  DATA <== "data: TLS 1.3, pinned, token" ==> APLUG
```

**Layering rule:** `continuum-proto` knows nothing about sockets, GTK or D-Bus. `continuumd`
knows nothing about GTK or GNOME. GNOME-specific code is in the UI and extensions only. This is
what keeps the system easy to change (NFR-MNT-5).

## Connection lifecycle
```mermaid
stateDiagram-v2
  [*] --> Discovered: mDNS seen
  Discovered --> Connecting: paired peer, or user picked it
  Connecting --> Handshake: TCP up
  Handshake --> Pairing: cert not pinned
  Handshake --> Connected: cert matches pin
  Handshake --> Rejected: pinned but cert differs
  Pairing --> Connected: QR secret or SAS confirmed
  Pairing --> Closed: timeout / refused
  Connected --> Backoff: socket error / 60 s no pong
  Backoff --> Connecting: retry (1, 2, 4 … 30 s)
  Rejected --> [*]: warn user, never retry automatically
  Closed --> [*]
```

## A typical exchange (phone sends a file)
```mermaid
sequenceDiagram
  participant P as Phone (share plugin)
  participant D as continuumd
  participant U as GNOME
  P->>D: share.offer {transfer_id, files}
  D->>P: share.accept {transfer_id, port, token}
  P->>D: new TLS conn: token, file_index, offset
  loop chunks
    P->>D: bytes (written to .part)
  end
  D->>D: verify SHA-256, rename .part → final
  D->>U: notification "Open" / "Show in folder"
```

## Concurrency model
- **Desktop:** one tokio runtime in `continuumd`. One task per connection. Plugins receive
  messages over channels, so a slow or panicking plugin can't block the connection (NFR-REL-3).
  The GTK app has its own main loop and only talks D-Bus.
- **Android:** the foreground service owns a coroutine scope. One coroutine per connection,
  `SupervisorJob` so one plugin's failure doesn't cancel the others.

## Persistence
| Data | Desktop | Android |
|---|---|---|
| Private key | `~/.local/share/continuum/identity/` (`0600`) | Android Keystore |
| Paired devices (name, type, pinned fingerprint, settings) | `devices.toml`, atomic write | DataStore |
| Partial transfers | `<dest>/.name.part` + `.name.part.json` (verified offset) | app cache dir |
