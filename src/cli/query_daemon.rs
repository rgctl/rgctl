//! Ephemeral query daemon — keeps mmap graph + blast engine warm across CLI invocations.
//!
//! Protocol: newline-delimited JSON-RPC-like messages over a local transport:
//! - Unix: domain socket at `<repo>/.rbuilder/query.sock`
//! - Windows: loopback TCP; port stored in `<repo>/.rbuilder/query.port`

use super::blast_radius::{build_lite_response, BlastRadiusArgs};
use super::blast_radius_output::BlastRadiusResponse;
use super::context::CliContext;
use crate::analysis::{parse_fqn_symbol, try_load_engine, BlastRadiusEngine};
use anyhow::{Context, Result};
use rbuilder_graph::SnapshotNodeStore;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(200);

/// Default local endpoint path under a repository root.
#[cfg(unix)]
pub fn default_socket_path(repo: &Path) -> PathBuf {
    repo.join(".rbuilder").join("query.sock")
}

/// Default port-file path under a repository root (Windows loopback daemon).
#[cfg(windows)]
pub fn default_socket_path(repo: &Path) -> PathBuf {
    repo.join(".rbuilder").join("query.port")
}

fn daemon_disabled() -> bool {
    std::env::var_os("RBUILDER_NO_QUERY_DAEMON").is_some()
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcRequest {
    id: u64,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct RpcResponse {
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BlastRadiusParams {
    symbol: String,
    #[serde(default)]
    class: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    graph_digest: Option<String>,
}

struct DaemonState {
    repo: PathBuf,
    digest: Arc<str>,
    store: Arc<SnapshotNodeStore>,
    engine: Mutex<BlastRadiusEngine>,
}

impl DaemonState {
    fn handle(&self, request: RpcRequest) -> RpcResponse {
        match request.method.as_str() {
            "ping" => RpcResponse {
                id: request.id,
                ok: true,
                result: Some(serde_json::json!({
                    "graph_digest": self.digest.as_ref(),
                    "node_count": self.store.node_count(),
                })),
                error: None,
            },
            "blast_radius" => match self.blast_radius(request.id, &request.params) {
                Ok(value) => RpcResponse {
                    id: request.id,
                    ok: true,
                    result: Some(value),
                    error: None,
                },
                Err(err) => RpcResponse {
                    id: request.id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                },
            },
            other => RpcResponse {
                id: request.id,
                ok: false,
                result: None,
                error: Some(format!("unknown method '{other}'")),
            },
        }
    }

    fn blast_radius(&self, _id: u64, params: &serde_json::Value) -> Result<serde_json::Value> {
        let params: BlastRadiusParams =
            serde_json::from_value(params.clone()).context("invalid blast_radius params")?;
        if let Some(digest) = &params.graph_digest {
            if digest != self.digest.as_ref() {
                anyhow::bail!("graph digest mismatch (run discover and restart serve)");
            }
        }
        let parsed = parse_fqn_symbol(&params.symbol, params.class.clone(), params.file.clone());
        let args = BlastRadiusArgs {
            symbol: params.symbol,
            depth: params.depth,
            policy_file: None,
            no_policy: true,
            with_slices: false,
            class: parsed.class_filter.clone(),
            file: parsed.file_filter.clone(),
        };
        let ctx = CliContext::new(
            Some(self.repo.clone()),
            None,
            super::OutputFormat::Json,
            None,
            false,
        );
        let engine = self.engine.lock().expect("engine mutex poisoned");
        let response = build_lite_response(&ctx, &args, &parsed, &self.store, &engine)?;
        Ok(serde_json::to_value(response)?)
    }
}

fn write_response(stream: &mut impl Write, response: &RpcResponse) -> Result<()> {
    let line = serde_json::to_string(response)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

fn handle_connection<S: Read + Write>(state: &Arc<DaemonState>, mut stream: S) -> Result<()> {
    loop {
        let line = {
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                break;
            }
            line
        };
        if line.len() > MAX_LINE_BYTES {
            anyhow::bail!("request line exceeds {MAX_LINE_BYTES} bytes");
        }
        let request: RpcRequest =
            serde_json::from_str(line.trim()).context("invalid JSON request line")?;
        let response = state.handle(request);
        write_response(&mut stream, &response)?;
    }
    Ok(())
}

fn load_daemon_state(ctx: &CliContext) -> Result<Arc<DaemonState>> {
    let session = ctx
        .snapshot_session()?
        .context("graph snapshot not found (run `rbuilder discover` first)")?;
    let engine = try_load_engine(&ctx.repo, session.digest.as_ref())?.context(
        "blast engine snapshot not found or digest mismatch (run `rbuilder discover` first)",
    )?;
    Ok(Arc::new(DaemonState {
        repo: ctx.repo.clone(),
        digest: session.digest,
        store: session.store,
        engine: Mutex::new(engine),
    }))
}

fn rpc_call<S: Read + Write>(stream: &mut S, request: &RpcRequest) -> Result<RpcResponse> {
    let line = serde_json::to_string(request)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(&mut *stream);
    let mut response_line = String::new();
    let bytes = reader.read_line(&mut response_line)?;
    if bytes == 0 {
        anyhow::bail!("query daemon closed connection without response");
    }
    serde_json::from_str(response_line.trim()).context("invalid JSON response from query daemon")
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    pub fn serve(ctx: &CliContext, socket_path: PathBuf, idle_secs: u64) -> Result<()> {
        let state = load_daemon_state(ctx)?;
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)
                .with_context(|| format!("remove stale socket {}", socket_path.display()))?;
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("bind query socket {}", socket_path.display()))?;
        eprintln!(
            "rbuilder query daemon listening on {} (idle exit {}s)",
            socket_path.display(),
            idle_secs
        );

        run_accept_loop(state, listener, idle_secs, || {
            let _ = std::fs::remove_file(&socket_path);
        })
    }

    pub fn rpc_call_endpoint(path: &Path, request: &RpcRequest) -> Result<Option<RpcResponse>> {
        let Some(mut stream) = connect_socket(path)? else {
            return Ok(None);
        };
        Ok(Some(super::rpc_call(&mut stream, request)?))
    }

    fn connect_socket(path: &Path) -> Result<Option<UnixStream>> {
        let stream = match UnixStream::connect(path) {
            Ok(stream) => stream,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        stream
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .context("set_read_timeout on query client")?;
        stream
            .set_write_timeout(Some(CONNECT_TIMEOUT))
            .context("set_write_timeout on query client")?;
        Ok(Some(stream))
    }

    fn run_accept_loop(
        state: Arc<DaemonState>,
        listener: UnixListener,
        idle_secs: u64,
        cleanup: impl FnOnce(),
    ) -> Result<()> {
        let idle_limit = Duration::from_secs(idle_secs);
        listener
            .set_nonblocking(true)
            .context("set_nonblocking on query listener")?;
        let mut last_activity = Instant::now();

        loop {
            if last_activity.elapsed() >= idle_limit {
                eprintln!("rbuilder query daemon exiting after idle timeout");
                break;
            }
            match listener.accept() {
                Ok((stream, _addr)) => {
                    last_activity = Instant::now();
                    if let Err(err) = handle_connection(&state, stream) {
                        eprintln!("query daemon connection error: {err:#}");
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err.into()),
            }
        }

        drop(listener);
        cleanup();
        Ok(())
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    pub fn serve(ctx: &CliContext, port_file: PathBuf, idle_secs: u64) -> Result<()> {
        let state = load_daemon_state(ctx)?;
        if let Some(parent) = port_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = TcpListener::bind("127.0.0.1:0").context("bind loopback query listener")?;
        let port = listener
            .local_addr()
            .context("query listener local_addr")?
            .port();
        std::fs::write(&port_file, port.to_string())
            .with_context(|| format!("write query port file {}", port_file.display()))?;

        eprintln!(
            "rbuilder query daemon listening on 127.0.0.1:{port} (port file {}, idle exit {}s)",
            port_file.display(),
            idle_secs
        );

        run_accept_loop(state, listener, idle_secs, || {
            let _ = std::fs::remove_file(&port_file);
        })
    }

    pub fn rpc_call_endpoint(path: &Path, request: &RpcRequest) -> Result<Option<RpcResponse>> {
        let Some(mut stream) = connect_stream(path)? else {
            return Ok(None);
        };
        Ok(Some(super::rpc_call(&mut stream, request)?))
    }

    fn connect_stream(port_file: &Path) -> Result<Option<TcpStream>> {
        if !port_file.is_file() {
            return Ok(None);
        }
        let port = std::fs::read_to_string(port_file)
            .with_context(|| format!("read query port file {}", port_file.display()))?
            .trim()
            .parse::<u16>()
            .with_context(|| format!("parse query port file {}", port_file.display()))?;
        let stream = match TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => stream,
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => return Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        stream
            .set_read_timeout(Some(CONNECT_TIMEOUT))
            .context("set_read_timeout on query client")?;
        stream
            .set_write_timeout(Some(CONNECT_TIMEOUT))
            .context("set_write_timeout on query client")?;
        Ok(Some(stream))
    }

    fn run_accept_loop(
        state: Arc<DaemonState>,
        listener: TcpListener,
        idle_secs: u64,
        cleanup: impl FnOnce(),
    ) -> Result<()> {
        let idle_limit = Duration::from_secs(idle_secs);
        listener
            .set_nonblocking(true)
            .context("set_nonblocking on query listener")?;
        let mut last_activity = Instant::now();

        loop {
            if last_activity.elapsed() >= idle_limit {
                eprintln!("rbuilder query daemon exiting after idle timeout");
                break;
            }
            match listener.accept() {
                Ok((stream, _addr)) => {
                    last_activity = Instant::now();
                    if let Err(err) = handle_connection(&state, stream) {
                        eprintln!("query daemon connection error: {err:#}");
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err.into()),
            }
        }

        drop(listener);
        cleanup();
        Ok(())
    }
}

/// Run the query daemon until idle timeout or fatal error.
pub fn serve(ctx: &CliContext, endpoint_path: PathBuf, idle_secs: u64) -> Result<()> {
    transport::serve(ctx, endpoint_path, idle_secs)
}

/// Try a warm-engine blast-radius query via the local query daemon.
pub fn try_client_blast_radius(
    ctx: &CliContext,
    args: &BlastRadiusArgs,
    parsed: &crate::analysis::ParsedSymbol,
    graph_digest: &str,
) -> Result<Option<BlastRadiusResponse>> {
    if daemon_disabled() {
        return Ok(None);
    }
    if args.with_slices || args.policy_file.is_some() {
        return Ok(None);
    }

    let endpoint = default_socket_path(&ctx.repo);
    let request = RpcRequest {
        id: 1,
        method: "blast_radius".into(),
        params: serde_json::json!({
            "symbol": args.symbol,
            "class": parsed.class_filter,
            "file": parsed.file_filter,
            "depth": args.depth,
            "graph_digest": graph_digest,
        }),
    };

    let Some(response) = transport::rpc_call_endpoint(&endpoint, &request)? else {
        return Ok(None);
    };
    if !response.ok {
        if let Some(err) = response.error {
            if err.contains("digest mismatch") {
                return Ok(None);
            }
            anyhow::bail!(err);
        }
        return Ok(None);
    }
    let value = response
        .result
        .context("query daemon returned empty result")?;
    Ok(Some(serde_json::from_value(value)?))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::analysis::BlastRadiusEngine;
    use crate::graph::backend::GraphBackend;
    use crate::graph::schema::{Edge, EdgeType, Node, NodeType};
    use crate::graph::{CodeGraph, PreparedGraphSnapshot};
    use std::os::unix::net::UnixStream;
    use std::thread;
    use tempfile::TempDir;

    fn seed_repo(dir: &Path) {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let mut ids = Vec::new();
        for i in 0..100 {
            let node = Node::new(NodeType::Function, format!("fn{i}"));
            ids.push(node.id);
            backend.insert_node(node).unwrap();
        }
        for i in 0..ids.len() {
            backend
                .insert_edge(Edge::new(ids[i], ids[(i + 1) % ids.len()], EdgeType::Calls))
                .unwrap();
        }
        let prepared = PreparedGraphSnapshot::from_backend(backend).unwrap();
        let digest = prepared.content_digest.clone();
        let rb = dir.join(".rbuilder");
        std::fs::create_dir_all(&rb).unwrap();
        prepared
            .write_to_path(&rb.join("graph.snapshot.bin"))
            .unwrap();
        let engine = BlastRadiusEngine::build(backend).unwrap();
        engine
            .to_engine_snapshot(digest)
            .write_to_path(&rb.join("blast_engine.snapshot.bin"))
            .unwrap();
    }

    #[test]
    fn query_daemon_blast_radius_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        seed_repo(repo);

        let ctx = CliContext::new(
            Some(repo.to_path_buf()),
            None,
            super::super::OutputFormat::Json,
            None,
            false,
        );
        let state = load_daemon_state(&ctx).unwrap();
        let (mut client, server) = UnixStream::pair().unwrap();
        let state_thread = Arc::clone(&state);
        let server = thread::spawn(move || handle_connection(&state_thread, server).unwrap());

        let request = RpcRequest {
            id: 1,
            method: "blast_radius".into(),
            params: serde_json::json!({
                "symbol": "fn50",
                "graph_digest": state.digest.as_ref(),
            }),
        };
        let response = rpc_call(&mut client, &request).unwrap();
        assert!(response.ok, "{:?}", response.error);
        let value: BlastRadiusResponse = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(value.target.symbol, "fn50");

        drop(client);
        server.join().unwrap();
    }
}
