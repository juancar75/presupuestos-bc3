// SPDX-License-Identifier: GPL-3.0-or-later
//! Pruebas del servidor MCP: protocolo y herramientas con cifras calculadas a mano.

use ppto_mcp::{Estado, procesar};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn llamar(e: &mut Estado, nombre: &str, args: Value) -> (String, bool) {
    let r = procesar(
        e,
        &json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":nombre,"arguments":args}}),
    )
    .unwrap();
    let res = &r["result"];
    (
        res["content"][0]["text"].as_str().unwrap().to_owned(),
        res["isError"].as_bool().unwrap(),
    )
}

#[test]
fn protocolo_basico() {
    let mut e = Estado::default();
    let r = procesar(
        &mut e,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
    )
    .unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "presupuestos-bc3");
    // Las notificaciones no tienen respuesta
    assert!(procesar(&mut e, &json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    let l = procesar(&mut e, &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
    let nombres: Vec<&str> = l["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(nombres.contains(&"abrir_presupuesto") && nombres.contains(&"simular_subcontrata"));
    for t in l["result"]["tools"].as_array().unwrap() {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }
    let x = procesar(&mut e, &json!({"jsonrpc":"2.0","id":3,"method":"no/existe"})).unwrap();
    assert_eq!(x["error"]["code"], -32601);
}

#[test]
fn sin_presupuesto_abierto_se_explica() {
    let mut e = Estado::default();
    let (t, err) = llamar(&mut e, "resumen", json!({}));
    assert!(err && t.contains("abrir_presupuesto"));
}

#[test]
fn consultas_sobre_el_ejemplo() {
    let mut e = Estado::default();
    llamar(&mut e, "abrir_ejemplo", json!({}));
    let (t, err) = llamar(&mut e, "resumen", json!({}));
    assert!(!err);
    assert!(
        t.contains("| C01 | Calefacción por suelo radiante | 7.005,15 | 100,00 |"),
        "{t}"
    );
    assert!(t.contains("**Total: 10.086,72 €**"), "{t}");

    // Búsqueda sin tildes ni mayúsculas
    let (t, _) = llamar(&mut e, "buscar", json!({"texto": "CALEFACCION oficial"}));
    assert!(
        t.contains("| MO.OF1F | h | Oficial 1ª instalador de calefacción | mano de obra | 24,50 |"),
        "{t}"
    );
    let (t, _) = llamar(&mut e, "buscar", json!({"texto": "zzz"}));
    assert!(t.starts_with("Sin resultados"));

    let (t, _) = llamar(&mut e, "ver_concepto", json!({"codigo": "SR.M2"}));
    assert!(t.contains("Coste directo unitario: 29,89 €/m2"), "{t}");
    assert!(t.contains("| C01 | 210,00 | 29,89 | 6.276,90 |"), "{t}");
    assert!(
        t.contains("| %MA | % | Medios auxiliares | 2,000 | 29,30 (base) | 0,59 |"),
        "{t}"
    );
    assert!(t.contains("**210,00 m2**"), "{t}");

    let (t, _) = llamar(&mut e, "recursos", json!({"naturaleza": "mano_obra"}));
    assert!(t.contains("Horas de mano de obra: 112,50 · importe 2.562,00 €"), "{t}");
    assert!(!t.contains("MT.TUBO16"));
}

#[test]
fn simulaciones_no_modifican_el_presupuesto() {
    let mut e = Estado::default();
    llamar(&mut e, "abrir_ejemplo", json!({}));
    // CI 45 % → PEM 10.157,37 (calculado a mano en las pruebas del motor)
    let (t, err) = llamar(&mut e, "simular_cambios", json!({"costes_indirectos": 45}));
    assert!(!err, "{t}");
    assert!(t.contains("| PEM | 7.005,15 | 10.157,37 | 3.152,22 (45,00 %) |"), "{t}");
    // Tubo 1,10 → 1,25 (texto con coma): SR.M2 30,96 → PEM 7.229,85
    let (t, _) = llamar(
        &mut e,
        "simular_cambios",
        json!({"precios": [{"codigo": "MT.TUBO16", "precio": "1,25"}]}),
    );
    assert!(t.contains("| PEM | 7.005,15 | 7.229,85 | 224,70"), "{t}");
    assert!(t.contains("- SR.M2 en C01: 224,70 €"), "{t}");
    // Cantidad de colectores 3 → 8: +5 × 242,75 = 1.213,75
    let (t, _) = llamar(
        &mut e,
        "simular_cambios",
        json!({"cantidades": [{"capitulo": "C01", "partida": "SR.COL8", "cantidad": 8}]}),
    );
    assert!(t.contains("| PEM | 7.005,15 | 8.218,90 | 1.213,75"), "{t}");
    // El presupuesto abierto sigue intacto
    assert_eq!(e.presupuesto.as_ref().unwrap(), &ppto_core::ejemplos::suelo_radiante());
    // Errores explicados
    let (t, err) = llamar(&mut e, "simular_cambios", json!({}));
    assert!(err && t.contains("ningún cambio"));
    let (t, err) = llamar(
        &mut e,
        "simular_cambios",
        json!({"precios": [{"codigo": "SR.M2", "precio": 1}]}),
    );
    assert!(err && t.contains("no es un recurso básico"), "{t}");
}

#[test]
fn subcontrata_por_metro_de_tubo_y_alzado() {
    let mut e = Estado::default();
    llamar(&mut e, "abrir_ejemplo", json!({}));
    // Oferta SUB-B: 1,60 €/m de tubo, 7 m por m² → 2.352,00; ahorro 37,80; 105 h liberadas
    let (t, err) = llamar(
        &mut e,
        "simular_subcontrata",
        json!({"partida": "SR.M2", "modalidad": "unidad", "precio": 1.6, "unidad": "m", "factor": 7}),
    );
    assert!(!err, "{t}");
    assert!(t.contains("| Coste propio retirado | 2.389,80 € |"), "{t}");
    assert!(t.contains("| Coste contratado | 2.352,00 € |"), "{t}");
    assert!(t.contains("| Equivale a | 11,20 €/m2 de partida |"), "{t}");
    assert!(t.contains("| **Ahorro** | **37,80 €** |"), "{t}");
    assert!(t.contains("Horas propias liberadas: 105,00"), "{t}");
    let (t, _) = llamar(
        &mut e,
        "simular_subcontrata",
        json!({"partida": "SR.M2", "modalidad": "alzado", "importe": 2300}),
    );
    assert!(t.contains("| **Ahorro** | **89,80 €** |"), "{t}");
}

#[test]
fn exportaciones_crean_ficheros_y_no_pisan_el_origen() {
    let dir = std::env::temp_dir().join(format!("ppto-mcp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut e = Estado::default();
    llamar(&mut e, "abrir_ejemplo", json!({}));
    let bc3 = dir.join("ejemplo.bc3");
    let (t, err) = llamar(&mut e, "exportar_bc3", json!({"ruta": bc3.to_string_lossy()}));
    assert!(!err, "{t}");
    let xlsx = dir.join("ejemplo.xlsx");
    let (t, err) = llamar(&mut e, "exportar_excel", json!({"ruta": xlsx.to_string_lossy()}));
    assert!(!err, "{t}");
    assert!(xlsx.metadata().unwrap().len() > 1000);
    // Abrir el BC3 exportado y comprobar el PEM
    let (t, err) = llamar(&mut e, "abrir_presupuesto", json!({"ruta": bc3.to_string_lossy()}));
    assert!(!err && t.contains("0 errores") && t.contains("PEM 7.005,15 €"), "{t}");
    // No se puede sobrescribir el fichero de origen
    let (t, err) = llamar(&mut e, "exportar_bc3", json!({"ruta": bc3.to_string_lossy()}));
    assert!(err && t.contains("origen"), "{t}");
    let (t, err) = llamar(
        &mut e,
        "exportar_excel",
        json!({"ruta": dir.join("x.txt").to_string_lossy()}),
    );
    assert!(err && t.contains(".xlsx"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn binario_por_stdio() {
    let mut hijo = Command::new(env!("CARGO_BIN_EXE_ppto-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut entrada = hijo.stdin.take().unwrap();
    let mut salida = BufReader::new(hijo.stdout.take().unwrap());
    fn pedir(entrada: &mut impl Write, salida: &mut impl BufRead, m: Value) -> Value {
        writeln!(entrada, "{m}").unwrap();
        entrada.flush().unwrap();
        let mut l = String::new();
        salida.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
    let r = pedir(
        &mut entrada,
        &mut salida,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
    );
    assert_eq!(r["id"], 1);
    // Notificación: sin respuesta (si respondiera, la siguiente lectura fallaría)
    writeln!(
        entrada,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"})
    )
    .unwrap();
    pedir(
        &mut entrada,
        &mut salida,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"abrir_ejemplo","arguments":{}}}),
    );
    let r = pedir(
        &mut entrada,
        &mut salida,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"resumen","arguments":{}}}),
    );
    assert_eq!(r["id"], 3);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("7.005,15"));
    drop(entrada);
    assert!(hijo.wait().unwrap().success());
}
