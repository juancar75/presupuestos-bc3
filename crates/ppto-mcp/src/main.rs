// SPDX-License-Identifier: GPL-3.0-or-later
//! `ppto-mcp` — servidor MCP por entrada/salida estándar (stdio).
//!
//! Configuración en Claude Desktop (`claude_desktop_config.json`):
//! ```json
//! { "mcpServers": { "presupuestos": { "command": "C:\\ruta\\ppto-mcp.exe" } } }
//! ```
//! Opcionalmente, una ruta como argumento abre ese presupuesto al arrancar.

use std::io::{BufRead, Write};

fn main() {
    let mut estado = ppto_mcp::Estado::default();
    if let Some(ruta) = std::env::args().nth(1) {
        if let Err(e) = estado.abrir(&ruta) {
            eprintln!("ppto-mcp: {e}");
        }
    }
    let entrada = std::io::stdin().lock();
    let mut salida = std::io::stdout().lock();
    for linea in entrada.lines() {
        let Ok(linea) = linea else { break };
        if linea.trim().is_empty() {
            continue;
        }
        if let Some(respuesta) = ppto_mcp::procesar_linea(&mut estado, &linea) {
            if writeln!(salida, "{respuesta}").and_then(|()| salida.flush()).is_err() {
                break;
            }
        }
    }
}
