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
fn no_valora_el_ejemplo_ni_un_presupuesto_vacio() {
    let dir = std::env::temp_dir().join(format!("mcp-ejemplo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let activa = dir.join("obra-activa.json");
    let mut e = Estado {
        ruta_obra_activa: Some(activa.clone()),
        ruta_bandeja: Some(dir.join("propuestas.json")),
        ..Estado::default()
    };
    // El ejemplo marcado por el programa
    let p = ppto_core::ejemplos::suelo_radiante();
    let datos = std::collections::BTreeMap::from([(ppto_propuestas::DATO_EJEMPLO.to_owned(), "1".to_owned())]);
    ppto_propuestas::ObraActiva::guardar_con(&p, &datos, &activa).unwrap();
    let (t, err) = llamar(&mut e, "abrir_obra_activa", json!({}));
    assert!(err && t.contains("EJEMPLO"), "{t}");
    // Presupuesto vacío (pantalla de inicio)
    let vacio = ppto_core::Presupuesto::new("Presupuesto nuevo", "OBRA", "Presupuesto nuevo");
    ppto_propuestas::ObraActiva::guardar_con(&vacio, &Default::default(), &activa).unwrap();
    let (t, err) = llamar(&mut e, "abrir_obra_activa", json!({}));
    assert!(err && t.contains("vacío"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
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
    let (t, err) = llamar(&mut e, "comprobar_recursos", json!({}));
    assert!(
        !err && t.contains("Comprobación antes de valorar") && t.contains("1 sin precio"),
        "{t}"
    );
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
    assert!(t.contains("| 1 | P.VAL | IA | pendiente | media |"), "{t}");
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
    // Contradictorio: partida nueva; «nueva» como objeto o como texto JSON
    for nueva in [
        json!({"capitulo":"ADIC","capitulo_resumen":"ADICIONALES","despues_de":"C01","unidad":"ud","resumen":"Vaciado","cantidad":1}),
        json!(r#"{"capitulo":"ADIC","unidad":"ud","resumen":"Vaciado","cantidad":1}"#),
    ] {
        let (t, err) = llamar(
            &mut e,
            "proponer",
            json!({
                "partida": "ADIC01",
                "lineas": [{"codigo":"MO.OF1F","rendimiento":2}],
                "confianza": "media",
                "justificacion": "x",
                "nueva": nueva
            }),
        );
        assert!(!err && t.contains("ADIC01") && t.contains("49,00"), "{t}");
    }
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

#[test]
fn biblioteca_aprende_de_una_obra_y_de_tarifas_y_ofertas() {
    let dir = std::env::temp_dir().join(format!("mcp-bib-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.fijar_tarifa("MT.PANEL", "ACME", rust_decimal_macros::dec!(20))
        .unwrap();
    p.fijar_descuento("ACME", rust_decimal_macros::dec!(50)).unwrap();
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_bandeja: Some(dir.join("propuestas.json")),
        ruta_tarifas: Some(dir.join("tarifas.json")),
        ruta_conversaciones: Some(dir.join("asistente.json")),
        ruta_biblioteca: Some(dir.join("biblioteca.json")),
        ..Estado::default()
    };
    llamar(&mut e, "abrir_obra_activa", json!({}));
    let (t, err) = llamar(&mut e, "guardar_en_biblioteca", json!({ "origen": "real" }));
    assert!(!err && t.contains("2 nuevas") && t.contains("ACME"), "{t}");

    // Tarifa 2027 de ACME: el panel sube de 20 a 22 y conserva el 50 %
    let (t, err) = llamar(
        &mut e,
        "registrar_tarifa",
        json!({ "fabricante": "Acme", "etiqueta": "2027", "anio": 2027,
                "lineas": [{ "referencia": "MT.PANEL", "descripcion": "Panel tetones", "pvp": 22 }] }),
    );
    assert!(!err && t.contains("1 referencias"), "{t}");
    let (t, err) = llamar(&mut e, "buscar_biblioteca", json!({ "texto": "suelo radiante" }));
    assert!(
        !err && t.contains("SR.M2") && t.contains("| 11,00 |") && t.contains("tarifa 2027"),
        "{t}"
    );

    // Consultas sueltas a un distribuidor (sintéticas): se suman sin borrar la tarifa
    let precios = |refs: Vec<(&str, f64)>| {
        json!({ "fabricante": "DISTRIB", "etiqueta": "2026", "fuente": "Web del distribuidor 19-11-2026",
                "lineas": refs.iter().map(|(r, p)| json!({"referencia": r, "pvp": p, "grupo": "TUBO"})).collect::<Vec<_>>() })
    };
    let (t, err) = llamar(&mut e, "guardar_precios", precios(vec![("A1", 10.0), ("A2", 20.0)]));
    assert!(!err && t.contains("2 nuevos") && t.contains("real"), "{t}");
    let (t, err) = llamar(&mut e, "guardar_precios", precios(vec![("A2", 21.0), ("A3", 5.0)]));
    assert!(!err && t.contains("1 nuevos, 1 cambiados"), "{t}");
    let b = ppto_biblioteca::Biblioteca::cargar(&dir.join("biblioteca.json"));
    let t26 = &b.fabricante("DISTRIB").unwrap().tarifas[0];
    assert_eq!(t26.lineas.len(), 3, "no borra A1");
    assert_eq!(t26.lineas["A1"].origen, ppto_biblioteca::Origen::Real);
    assert_eq!(
        t26.lineas["A1"].fuente.as_deref(),
        Some("Web del distribuidor 19-11-2026")
    );
    let (r, _, _) = b.recuento_origen();
    assert!(r >= 4, "la tarifa ACME 2027 también cuenta como cierta");
    let (t, err) = llamar(
        &mut e,
        "guardar_precios",
        json!({ "fabricante": "X", "fuente": "", "lineas": [] }),
    );
    assert!(err && t.contains("fuente"), "{t}");

    // Oferta en netos de otro fabricante
    let (t, err) = llamar(
        &mut e,
        "registrar_oferta",
        json!({ "fabricante": "LG", "numero": "OF-1", "fecha": "2026-10-18",
                "lineas": [{ "referencia": "ARUM100", "descripcion": "Exterior VRF", "neto": 4800 }] }),
    );
    assert!(!err && t.contains("1 líneas"), "{t}");
    let (t, _) = llamar(&mut e, "buscar_biblioteca", json!({ "texto": "vrf" }));
    assert!(t.contains("oferta OF-1") && t.contains("neto"), "{t}");

    let (t, err) = llamar(
        &mut e,
        "fijar_descuento_biblioteca",
        json!({ "fabricante": "LG", "porcentaje": 50 }),
    );
    assert!(!err, "{t}");
    // Descuento encadenado (hasta tres escalones)
    let (t, err) = llamar(
        &mut e,
        "fijar_descuento_biblioteca",
        json!({ "fabricante": "Fabb", "porcentaje": "50+10+5" }),
    );
    assert!(!err, "{t}");
    let (t, err) = llamar(
        &mut e,
        "fijar_descuento_biblioteca",
        json!({ "fabricante": "Fabb", "porcentaje": "40+8+5+2" }),
    );
    assert!(err && t.contains("como mucho 3"), "{t}");
    let (t, _) = llamar(&mut e, "estado_biblioteca", json!({}));
    assert!(
        t.contains("2 partidas") && t.contains("| ACME | 2027 |") && t.contains("| LG |"),
        "{t}"
    );
    assert!(t.contains("| FABB |") && t.contains("50+10+5 %"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn datos_y_documentos_de_la_obra() {
    use std::collections::BTreeMap;
    let dir = std::env::temp_dir().join(format!("mcp-docs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("documentos");
    std::fs::create_dir_all(docs.join("Memorias")).unwrap();
    // Documentos sintéticos
    let memoria = "Memoria sintética. Garaje de 2 plantas bajo rasante con 60 plazas.".repeat(2000);
    std::fs::write(docs.join("Memorias").join("memoria.txt"), &memoria).unwrap();
    std::fs::write(docs.join("plano.dwg"), b"x").unwrap();
    std::fs::write(
        docs.join("planta.dxf"),
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n6\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nINSERT\n8\nPCI\n2\nDET\n10\n0\n20\n0\n\
         0\nINSERT\n8\nPCI\n2\nDET\n10\n7\n20\n0\n\
         0\nTEXT\n8\nLEYENDA\n1\nDetector óptico\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    std::fs::write(dir.join("fuera.txt"), b"secreto").unwrap();
    let p = ppto_core::ejemplos::suelo_radiante();
    let activa = dir.join("obra-activa.json");
    let datos = BTreeMap::from([
        ("obra.documentos".to_owned(), docs.display().to_string()),
        ("obra.direccion".to_owned(), "C/ Sintética 1".to_owned()),
        ("especialidad".to_owned(), "ventilacion_garaje".to_owned()),
        ("ficha.garaje_altura".to_owned(), "2.7".to_owned()),
    ]);
    ppto_propuestas::ObraActiva::guardar_con(&p, &datos, &activa).unwrap();
    // Guardar solo el presupuesto conserva los datos de esa obra
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_ficha: Some(dir.join("ficha_propuesta.json")),
        ..Estado::default()
    };
    // Al abrir la obra, si hay documentos y faltan datos, se pide leerlos antes de valorar
    let (t, err) = llamar(&mut e, "abrir_obra_activa", json!({}));
    assert!(
        !err && t.contains("ANTES de valorar") && t.contains("proponer_datos_obra"),
        "{t}"
    );
    let (t, err) = llamar(&mut e, "ver_datos_obra", json!({}));
    assert!(
        !err && t.contains("C/ Sintética 1") && t.contains("| garaje_plazas |"),
        "{t}"
    );
    assert!(t.contains("2.7 m"), "{t}");
    let (t, err) = llamar(&mut e, "documentos_obra", json!({}));
    assert!(
        !err && t.contains("memoria.txt") && t.contains("guardarlo como DXF"),
        "{t}"
    );
    assert!(
        t.contains("01 Proyecto: no existe") && t.contains("Crear carpetas"),
        "{t}"
    );
    let (t, err) = llamar(&mut e, "leer_documento", json!({"ruta": "Memorias/memoria.txt"}));
    assert!(!err && t.contains("60 plazas") && t.contains("desde = 60000"), "{t}");
    let (t, _) = llamar(
        &mut e,
        "leer_documento",
        json!({"ruta": "Memorias/memoria.txt", "desde": 60000}),
    );
    assert!(t.contains("plazas") && t.contains("desde = 120000"), "{t}");
    let (t, err) = llamar(&mut e, "leer_plano", json!({"ruta": "planta.dxf", "bloque": "DET"}));
    assert!(
        !err && t.contains("| DET | PCI | 2 |") && t.contains("mediana 7.0 m") && t.contains("Detector óptico"),
        "{t}"
    );
    let (t, err) = llamar(&mut e, "leer_plano", json!({"ruta": "plano.dwg"}));
    assert!(err && t.contains("DXF"), "{t}"); // sin LibreDWG: explica cómo
    let (t, err) = llamar(&mut e, "leer_documento", json!({"ruta": "../fuera.txt"}));
    assert!(err && t.contains("carpeta de la obra"), "{t}");
    let (t, err) = llamar(
        &mut e,
        "proponer_datos_obra",
        json!({"datos": [
            {"clave": "garaje_plazas", "valor": 60, "fuente": "Memoria, pág. 1"},
            {"clave": "plantas_bajo", "valor": "2", "fuente": "Memoria, pág. 1"},
            {"clave": "municipio", "valor": "Madrid", "fuente": "Memoria"},
            {"clave": "garaje_altura", "valor": "alta", "fuente": "x"},
            {"clave": "inventada", "valor": 1, "fuente": "x"}
        ]}),
    );
    assert!(
        !err && t.starts_with("3 datos propuestos") && t.contains("no es un número"),
        "{t}"
    );
    let f = ppto_propuestas::FichaPropuesta::cargar(&dir.join("ficha_propuesta.json"));
    let v = f.obras.values().next().unwrap();
    assert_eq!(v.len(), 3);
    let (t, _) = llamar(&mut e, "ver_datos_obra", json!({}));
    assert!(t.contains("3 datos propuestos pendientes"), "{t}");
    // Comparar con obras parecidas: sin obras guardadas, lo dice
    e.ruta_biblioteca = Some(dir.join("biblioteca.json"));
    let (t, err) = llamar(&mut e, "comparar_obras", json!({}));
    assert!(!err && t.contains("Aún no hay obras del mismo tipo"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn proponer_cambios_de_la_obra() {
    let dir = std::env::temp_dir().join(format!("mcp-cambios-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.nueva_partida("C01", "P.INERCIA", "ud", "Depósito de inercia")
        .unwrap();
    p.fijar_precio("P.INERCIA", rust_decimal_macros::dec!(500)).unwrap();
    p.nueva_partida("C01", "P.LINEA", "m", "Línea frigorífica").unwrap();
    p.fijar_precio("P.LINEA", rust_decimal_macros::dec!(10)).unwrap();
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
        "proponer_cambios",
        json!({
            "confianza": "alta",
            "justificacion": "Indicación del usuario",
            "cambios": [
                { "tipo": "borrar", "partida": "P.INERCIA" },
                { "tipo": "cantidad", "partida": "P.LINEA", "medicion": [
                    { "comentario": "Cubierta", "uds": 1, "largo": 10 },
                    { "comentario": "Patinillo", "uds": 1, "largo": "19,5" },
                    { "comentario": "Falso techo", "uds": 1, "largo": 20 }
                ] },
                { "tipo": "texto", "partida": "P.LINEA", "resumen": "Línea frigorífica 3 tubos" }
            ]
        }),
    );
    assert!(!err, "{t}");
    assert!(
        t.contains("3 cambios") && t.contains("Quita P.INERCIA") && t.contains("49.5"),
        "{t}"
    );
    // No cambia la obra: queda en la bandeja
    let b = ppto_propuestas::Bandeja::cargar(&bandeja);
    assert_eq!(b.entradas.len(), 1);
    let prop = &b.entradas[0].propuesta;
    assert!(matches!(prop.contenido, ppto_core::propuesta::Contenido::Cambios(ref c) if c.len() == 3));
    let mut q = p.clone();
    let a = q.aplicar_propuesta(prop).unwrap();
    // PEM: −500 (inercia) + 10 × 48,5 (línea de 1 a 49,5 m) = −15
    assert_eq!(a.pem_despues - a.pem_antes, rust_decimal_macros::dec!(-15));
    // Un cambio no válido: no se guarda nada
    let (t, err) = llamar(
        &mut e,
        "proponer_cambios",
        json!({ "justificacion": "x", "cambios": [
            { "tipo": "borrar", "partida": "P.INERCIA" },
            { "tipo": "borrar", "partida": "NO.EXISTE" }
        ] }),
    );
    assert!(err && t.contains("no se ha guardado nada"), "{t}");
    let (t, err) = llamar(
        &mut e,
        "proponer_cambios",
        json!({ "justificacion": "x", "cambios": [{ "tipo": "renombrar", "partida": "P.LINEA" }] }),
    );
    assert!(err && t.contains("no válido"), "{t}");
    assert_eq!(ppto_propuestas::Bandeja::cargar(&bandeja).entradas.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn aviso_de_materiales_a_neto() {
    let dir = std::env::temp_dir().join(format!("mcp-neto-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.nueva_partida("C01", "P.LINEA", "m", "Línea frigorífica").unwrap();
    p.fijar_precio("P.LINEA", rust_decimal_macros::dec!(10)).unwrap();
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let mut e = Estado {
        ruta_obra_activa: Some(activa),
        ruta_bandeja: Some(dir.join("propuestas.json")),
        ruta_tarifas: Some(dir.join("tarifas.json")),
        ruta_conversaciones: Some(dir.join("asistente.json")),
        ..Estado::default()
    };
    llamar(&mut e, "abrir_obra_activa", json!({}));
    let mo = json!({ "codigo": "MO.OF", "rendimiento": 0.5, "naturaleza": "mano_obra", "unidad": "h", "resumen": "Oficial", "precio": 30 });
    // A neto: aviso
    let (t, err) = llamar(
        &mut e,
        "proponer",
        json!({ "partida": "P.LINEA", "confianza": "alta", "justificacion": "x", "lineas": [
            { "codigo": "CU38", "rendimiento": 1, "naturaleza": "material", "unidad": "m", "resumen": "Cobre 3/8", "precio": 3.31 },
            mo.clone()
        ] }),
    );
    assert!(!err, "{t}");
    assert!(t.contains("materiales a precio neto, sin tarifa: CU38"), "{t}");
    // Con PVP y fabricante: sin aviso; los estimados tampoco avisan
    let (t, err) = llamar(
        &mut e,
        "proponer_lote",
        json!({ "confianza": "alta", "justificacion": "x",
            "descuentos": [{ "fabricante": "SALTOKI/COBRE", "porcentaje": 70 }],
            "recursos": [
                { "codigo": "SALTOKI.CU38", "naturaleza": "material", "unidad": "m", "resumen": "Cobre 3/8", "pvp": 11.04, "fabricante": "SALTOKI", "grupo": "COBRE" },
                { "codigo": "M.PEQ", "naturaleza": "material", "unidad": "ud", "resumen": "Pequeño material (estimado)", "precio": 15 },
                { "codigo": "CU12", "naturaleza": "material", "unidad": "m", "resumen": "Cobre 1/2", "precio": 4.51 },
                { "codigo": "MO.OF", "naturaleza": "mano_obra", "unidad": "h", "resumen": "Oficial", "precio": 30 }
            ],
            "propuestas": [
                { "partida": "P.LINEA", "lineas": [["SALTOKI.CU38", 1], ["M.PEQ", 0.1], ["MO.OF", 0.5]] },
                { "partida": "SR.COL8", "lineas": [["CU12", 1], ["MO.OF", 0.5]] }
            ] }),
    );
    assert!(!err, "{t}");
    assert!(t.contains("2 propuestas guardadas"), "{t}");
    assert!(t.contains("sin tarifa: CU12."), "solo CU12 queda a neto: {t}");
    assert!(!t.contains("SALTOKI.CU38.") && !t.contains("M.PEQ,"), "{t}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dos_agentes_proponen_comentan_y_discrepan() {
    let dir = std::env::temp_dir().join(format!("mcp-multiagente-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut p = ppto_core::ejemplos::suelo_radiante();
    p.nueva_partida("C01", "P.VAL", "ud", "Válvula de zona").unwrap();
    let activa = dir.join("obra-activa.json");
    ppto_propuestas::ObraActiva::guardar(&p, &activa).unwrap();
    let bandeja = dir.join("propuestas.json");
    let _ = std::fs::remove_file(&bandeja);
    let nuevo = |cliente: &str| {
        let mut e = Estado {
            ruta_obra_activa: Some(activa.clone()),
            ruta_bandeja: Some(bandeja.clone()),
            ruta_tarifas: Some(dir.join("tarifas.json")),
            ..Estado::default()
        };
        ppto_mcp::procesar(
            &mut e,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":cliente,"version":"1"}}}),
        );
        let (t, err) = llamar(&mut e, "abrir_obra_activa", json!({}));
        assert!(!err, "{t}");
        e
    };
    let mut claude = nuevo("claude-ai");
    let mut chatgpt = nuevo("codex-mcp-client");
    assert_eq!((claude.autor.as_str(), chatgpt.autor.as_str()), ("Claude", "ChatGPT"));
    let (t, err) = llamar(
        &mut claude,
        "proponer",
        json!({"partida":"P.VAL","lineas":[{"codigo":"MT.A","rendimiento":1,"naturaleza":"material","unidad":"ud","resumen":"Válvula","precio":100}],"confianza":"alta","justificacion":"tarifa"}),
    );
    assert!(!err, "{t}");
    let (t, err) = llamar(
        &mut chatgpt,
        "proponer",
        json!({"partida":"P.VAL","lineas":[{"codigo":"MT.B","rendimiento":1,"naturaleza":"material","unidad":"ud","resumen":"Válvula con actuador","precio":140}],"confianza":"media","justificacion":"incluye actuador"}),
    );
    assert!(!err, "{t}");
    let (t, err) = llamar(
        &mut claude,
        "comentar_propuesta",
        json!({"id":2,"texto":"El actuador ya está en otra partida.","de_acuerdo":false}),
    );
    assert!(!err && t.contains("Claude") && t.contains("en contra"), "{t}");
    let (t, _) = llamar(&mut chatgpt, "comentar_propuesta", json!({"id":99,"texto":"x"}));
    assert!(t.contains("no hay propuesta"), "{t}");
    let (t, err) = llamar(&mut chatgpt, "ver_propuestas", json!({}));
    assert!(!err, "{t}");
    assert!(t.contains("| Claude |") && t.contains("| ChatGPT |"), "{t}");
    assert!(
        t.contains("Desacuerdos") && t.contains("#1 Claude 100,00 €") && t.contains("#2 ChatGPT 140,00 €"),
        "{t}"
    );
    assert!(t.contains("#2 Claude (en contra): El actuador"), "{t}");
}
