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

#[test]
fn propuestas_a_la_obra_activa() {
    let dir = std::env::temp_dir().join(format!("mcp-propuestas-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.nueva_partida("C01", "P.VAL", "ud", "Válvula de zona").unwrap();
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_bandeja: Some(dir.join("propuestas.json")),
        ruta_tarifas: Some(dir.join("tarifas.json")),
        ..Estado::default()
    };
    // Sin abrir la obra activa no se propone
    llamar(&mut e, "abrir_ejemplo", json!({}));
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({"partida":"P.VAL","precio":10,"confianza":"alta","justificacion":"x"}),
    );
    assert!(err && t.contains("abrir_obra_activa"), "{t}");

    let (t, err) = llamar(&mut e, "abrir_obra_activa", json!({}));
    assert!(!err && t.contains("1 sin precio"), "{t}");
    let (t, _) = llamar(&mut e, "partidas_sin_precio", json!({}));
    assert!(t.contains("P.VAL") && t.contains("Válvula de zona"), "{t}");
    let (t, _) = llamar(&mut e, "buscar_tarifas", json!({"texto":"detector"}));
    assert!(t.contains("no tiene tarifas"), "{t}");

    // 0,5 h × 24,50 + válvula 30 + 2 % MA = 43,10
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({
            "partida": "P.VAL",
            "lineas": [
                {"codigo":"MO.OF1F","rendimiento":"0,5"},
                {"codigo":"MT.VAL","rendimiento":1,"naturaleza":"material","unidad":"ud","resumen":"Válvula de zona","precio":30},
                {"codigo":"%MA","rendimiento":2}
            ],
            "confianza": "media",
            "justificacion": "Rendimiento habitual; precio de catálogo"
        }),
    );
    assert!(
        !err && t.contains("#1") && t.contains("43,10") && t.contains("MT.VAL"),
        "{t}"
    );
    // El presupuesto del MCP no cambia
    let (t, _) = llamar(&mut e, "partidas_sin_precio", json!({}));
    assert!(t.contains("P.VAL"));
    let (t, _) = llamar(&mut e, "ver_propuestas", json!({}));
    assert!(t.contains("| 1 | P.VAL | pendiente | media |"), "{t}");
    // Material de tarifa con descuento estimado: 0,5 h × 24,50 + 600 × (1 − 0,45) = 342,25 + 2 % MA
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({
            "partida": "P.VAL",
            "lineas": [
                {"codigo":"MO.OF1F","rendimiento":"0,5"},
                {"codigo":"MT.CFK","rendimiento":1,"naturaleza":"material","unidad":"ud","resumen":"Compuerta cortafuegos","pvp":600,"fabricante":"Koolair"},
                {"codigo":"%MA","rendimiento":2}
            ],
            "descuentos": [{"fabricante":"KOOLAIR","porcentaje":45}],
            "confianza": "media",
            "justificacion": "PVP de tarifa Koolair"
        }),
    );
    // base 342,25 → 3,4225 → 3,423 × 2 = 6,846 → 6,85 → 349,10
    assert!(
        !err && t.contains("349,10") && t.contains("Descuento KOOLAIR 45"),
        "{t}"
    );
    // Sin descompuesto no se admite (siempre hay que descomponer), y no se guarda
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({"partida":"P.VAL","precio":5,"confianza":"baja","justificacion":"x"}),
    );
    assert!(err && t.contains("descompuesto"), "{t}");
    // Una propuesta imposible tampoco
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({"partida":"P.VAL","lineas":[{"codigo":"NO.EXISTE","rendimiento":1}],"confianza":"baja","justificacion":"x"}),
    );
    assert!(err && t.contains("no válida"), "{t}");
    let b = ppto_propuestas::Bandeja::cargar(&dir.join("propuestas.json"));
    assert_eq!(
        b.entradas.len(),
        1,
        "la segunda propuesta de P.VAL sustituye a la primera"
    );
}

#[test]
fn encargos_del_panel_del_asistente() {
    use ppto_propuestas::conversacion::{Autor, Conversaciones};
    let dir = std::env::temp_dir().join(format!("mcp-asistente-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = ppto_core::ejemplos::suelo_radiante();
    let obra = ppto_propuestas::clave(&p);
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let conv = dir.join("asistente.json");
    let oferta = dir.join("oferta fabricante.csv");
    std::fs::write(&oferta, "Ref;Descripción;PVP\nE08-100;Extractor;812,50\n").unwrap();
    Conversaciones::modificar(&conv, |c| {
        c.anadir(
            &obra,
            Autor::Usuario,
            "Valora con la oferta adjunta",
            vec![oferta.display().to_string()],
        )
    })
    .unwrap();
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_bandeja: Some(dir.join("propuestas.json")),
        ruta_tarifas: Some(dir.join("tarifas.json")),
        ruta_conversaciones: Some(conv.clone()),
        ..Estado::default()
    };
    // Abre la obra activa sola si hace falta
    let (t, err) = llamar(&mut e, "leer_mensajes", json!({}));
    assert!(
        !err && t.contains("Valora con la oferta adjunta") && t.contains("oferta fabricante.csv"),
        "{t}"
    );
    let (t, err) = llamar(&mut e, "leer_adjunto", json!({ "ruta": oferta.display().to_string() }));
    assert!(!err && t.contains("E08-100;Extractor;812,50"), "{t}");
    let (t, err) = llamar(&mut e, "leer_adjunto", json!({ "ruta": "x.dwg" }));
    assert!(err && t.contains("no admitido"), "{t}");

    let (t, err) = llamar(
        &mut e,
        "guardar_informe",
        json!({ "titulo": "Comparativa ofertas", "contenido": "# Comparativa\n" }),
    );
    assert!(!err && t.contains("Comparativa ofertas.md"), "{t}");
    let (t, err) = llamar(&mut e, "responder", json!({ "texto": "Hecho: 3 propuestas" }));
    assert!(!err, "{t}");
    let c = Conversaciones::cargar(&conv);
    assert!(c.pendientes(&obra).is_empty());
    assert_eq!(c.de_obra(&obra).last().unwrap().texto, "Hecho: 3 propuestas");
    let (t, _) = llamar(&mut e, "leer_mensajes", json!({}));
    assert!(
        t.contains("No hay mensajes pendientes") && t.contains("Hecho: 3 propuestas"),
        "{t}"
    );

    // Formato de la API de mensajes para el modo directo
    let h = ppto_mcp::herramientas_api(&["leer_mensajes", "responder"]);
    assert!(h.iter().all(|h| h["input_schema"]["type"] == "object"));
    assert!(!h.iter().any(|h| h["name"] == "responder"));
    assert!(h.iter().any(|h| h["name"] == "proponer"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn proponer_lote_con_recursos_comunes() {
    let dir = std::env::temp_dir().join(format!("mcp-lote-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.nueva_partida("C01", "P.A", "m", "Tubo A").unwrap();
    p.nueva_partida("C01", "P.B", "m", "Tubo B").unwrap();
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let bandeja = dir.join("propuestas.json");
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_bandeja: Some(bandeja.clone()),
        ruta_tarifas: Some(dir.join("tarifas.json")),
        ruta_conversaciones: Some(dir.join("asistente.json")),
        ..Estado::default()
    };
    llamar(&mut e, "abrir_obra_activa", json!({}));
    let (t, err) = llamar(
        &mut e,
        "proponer_lote",
        json!({
            "recursos": [
                { "codigo": "MT.TUBO", "naturaleza": "material", "unidad": "m", "resumen": "Tubo", "precio": 4 },
                { "codigo": "MO.OF1F", "naturaleza": "mano_obra", "unidad": "h", "resumen": "Oficial", "precio": 24.5 }
            ],
            "medios_auxiliares": 2,
            "confianza": "media",
            "justificacion": "estimación",
            "propuestas": [
                { "partida": "P.A", "lineas": [["MT.TUBO", 1.1], ["MO.OF1F", 0.2]] },
                { "partida": "P.B", "lineas": [{ "codigo": "MT.TUBO", "rendimiento": 1 }], "confianza": "alta" },
                { "partida": "NO.EXISTE", "lineas": [["MT.TUBO", 1]] }
            ]
        }),
    );
    assert!(!err, "{t}");
    assert!(
        t.contains("2 propuestas guardadas, 1 con error") && t.contains("NO.EXISTE"),
        "{t}"
    );
    let b = ppto_propuestas::Bandeja::cargar(&bandeja);
    assert_eq!(b.entradas.len(), 2);
    // %MA añadido y confianza propia respetada
    let pa = &b.entradas[0].propuesta;
    let ppto_core::propuesta::Contenido::Descompuesto(l) = &pa.contenido else {
        panic!()
    };
    assert!(l.iter().any(|x| x.codigo == "%MA"));
    assert_eq!(b.entradas[1].propuesta.confianza, ppto_core::propuesta::Confianza::Alta);
    // Más de 40: se rechaza entero
    let muchas: Vec<_> = (0..41)
        .map(|_| json!({ "partida": "P.A", "lineas": [["MT.TUBO", 1]] }))
        .collect();
    let (t, err) = llamar(
        &mut e,
        "proponer_lote",
        json!({ "confianza": "baja", "justificacion": "x", "propuestas": muchas }),
    );
    assert!(err && t.contains("como mucho 40"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}
