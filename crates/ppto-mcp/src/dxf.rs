// SPDX-License-Identifier: GPL-3.0-or-later
//! Planos en DXF (texto): bloques insertados por nombre y capa, textos de
//! las leyendas y, para un bloque, las distancias entre ellos (vecino más
//! cercano) y la superficie que ocupan, para comprobar coberturas
//! (detectores, BIE, extintores, rejillas…) frente a la medición.
//!
//! Lector propio y sencillo (pares código/valor de la sección ENTITIES):
//! no hace falta ninguna dependencia. Los DXF binarios y los DWG no se leen.
//!
//! Geometría del plano, no dinero: aquí sí se usa coma flotante (los importes
//! siguen en `Decimal` en todo el programa).
#![allow(clippy::float_arithmetic)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// Un bloque insertado (INSERT).
#[derive(Debug, Clone, PartialEq)]
pub struct Insercion {
    pub bloque: String,
    pub capa: String,
    pub x: f64,
    pub y: f64,
}

/// Lo leído del plano.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plano {
    /// `$INSUNITS`: 4 = mm, 5 = cm, 6 = m (0 = sin unidades).
    pub unidades: i32,
    pub inserciones: Vec<Insercion>,
    /// Textos (TEXT, MTEXT y atributos) con su capa.
    pub textos: Vec<(String, String)>,
    /// Entidades por capa (todas).
    pub por_capa: BTreeMap<String, usize>,
}

impl Plano {
    /// Metros por unidad del dibujo (si no lo dice, se supone mm si las
    /// coordenadas son grandes y m si no).
    pub fn escala(&self) -> f64 {
        match self.unidades {
            1 => 0.0254,
            2 => 0.3048,
            4 => 0.001,
            5 => 0.01,
            6 => 1.0,
            _ => {
                let max = self
                    .inserciones
                    .iter()
                    .map(|i| i.x.abs().max(i.y.abs()))
                    .fold(0.0, f64::max);
                if max > 5000.0 { 0.001 } else { 1.0 }
            }
        }
    }
}

fn decodificar(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_owned(),
        // DXF antiguos: ANSI (Windows-1252 ≈ Latin-1)
        Err(_) => b.iter().map(|&c| char::from(c)).collect(),
    }
}

/// Quita los códigos de formato de MTEXT (`\P`, `{\fArial|b0;…}`, `\H2.5;`).
fn limpiar_mtext(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' | '}' => {}
            '\\' => match it.next() {
                Some('P' | 'n') => o.push(' '),
                Some('~') => o.push(' '),
                Some('\\') => o.push('\\'),
                Some('{') => o.push('{'),
                Some('}') => o.push('}'),
                // \fArial|b0|i0;  \H2.5;  \C1;  \A1; … hasta el «;»
                Some(_) => {
                    for x in it.by_ref() {
                        if x == ';' {
                            break;
                        }
                    }
                }
                None => {}
            },
            c => o.push(c),
        }
    }
    o.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lee un DXF de texto.
pub fn leer(ruta: &Path) -> Result<Plano, String> {
    let b = std::fs::read(ruta).map_err(|e| format!("no se pudo leer {}: {e}", ruta.display()))?;
    if b.starts_with(b"AutoCAD Binary DXF") {
        return Err("DXF binario: guárdelo como DXF de texto (ASCII)".into());
    }
    leer_texto(&decodificar(&b))
}

/// Lee el contenido de un DXF de texto.
pub fn leer_texto(t: &str) -> Result<Plano, String> {
    let lineas: Vec<&str> = t.lines().collect();
    if lineas.len() < 4 {
        return Err("el fichero no parece un DXF".into());
    }
    let mut p = Plano::default();
    let mut i = 0;
    let par = |i: usize| -> Option<(i32, &str)> {
        let c = lineas.get(i)?.trim().parse::<i32>().ok()?;
        Some((c, lineas.get(i + 1).map_or("", |v| v.trim_end_matches('\r'))))
    };
    let mut seccion = String::new();
    let mut leer_unidades = false;
    // Entidad en curso
    let mut tipo = String::new();
    let mut campos: BTreeMap<i32, String> = BTreeMap::new();
    let mut mtext = String::new();
    let cerrar = |tipo: &str, campos: &BTreeMap<i32, String>, mtext: &str, p: &mut Plano| {
        if tipo.is_empty() {
            return;
        }
        let capa = campos.get(&8).cloned().unwrap_or_else(|| "0".into());
        *p.por_capa.entry(capa.clone()).or_default() += 1;
        let num = |k| campos.get(&k).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
        match tipo {
            "INSERT" => p.inserciones.push(Insercion {
                bloque: campos.get(&2).cloned().unwrap_or_default(),
                capa,
                x: num(10),
                y: num(20),
            }),
            "TEXT" | "ATTRIB" => {
                if let Some(v) = campos.get(&1).filter(|v| !v.trim().is_empty()) {
                    p.textos.push((v.trim().to_owned(), capa));
                }
            }
            "MTEXT" => {
                let v = limpiar_mtext(&format!("{mtext}{}", campos.get(&1).map_or("", String::as_str)));
                if !v.is_empty() {
                    p.textos.push((v, capa));
                }
            }
            _ => {}
        }
    };
    while let Some((c, v)) = par(i) {
        i += 2;
        match c {
            0 => {
                if seccion == "ENTITIES" {
                    cerrar(&tipo, &campos, &mtext, &mut p);
                }
                tipo.clear();
                campos.clear();
                mtext.clear();
                if v == "ENDSEC" {
                    seccion.clear();
                } else if seccion == "ENTITIES" {
                    v.clone_into(&mut tipo);
                }
            }
            2 if seccion.is_empty() => v.trim().clone_into(&mut seccion),
            9 if seccion == "HEADER" => leer_unidades = v.trim() == "$INSUNITS",
            70 if leer_unidades => {
                p.unidades = v.trim().parse().unwrap_or(0);
                leer_unidades = false;
            }
            // MTEXT largo: trozos en el código 3
            3 if tipo == "MTEXT" => mtext.push_str(v),
            _ if seccion == "ENTITIES" => {
                campos.entry(c).or_insert_with(|| v.to_owned());
            }
            _ => {}
        }
    }
    if p.por_capa.is_empty() {
        return Err("no se han encontrado entidades (¿DXF vacío o sin sección ENTITIES?)".into());
    }
    Ok(p)
}

/// Distancias al vecino más cercano (en metros) y superficie del rectángulo
/// que ocupan, de las inserciones de un bloque.
pub fn distancias(p: &Plano, bloque: &str) -> Option<(usize, f64, f64, f64, f64)> {
    let pts: Vec<(f64, f64)> = p
        .inserciones
        .iter()
        .filter(|i| i.bloque.eq_ignore_ascii_case(bloque))
        .map(|i| (i.x, i.y))
        .collect();
    if pts.len() < 2 {
        return None;
    }
    let e = p.escala();
    let mut d: Vec<f64> = pts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            pts.iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, b)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() * e)
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    d.sort_by(f64::total_cmp);
    let (xs, ys): (Vec<f64>, Vec<f64>) = pts.iter().copied().unzip();
    let ancho = (xs.iter().copied().fold(f64::MIN, f64::max) - xs.iter().copied().fold(f64::MAX, f64::min)) * e;
    let alto = (ys.iter().copied().fold(f64::MIN, f64::max) - ys.iter().copied().fold(f64::MAX, f64::min)) * e;
    Some((pts.len(), d[0], d[d.len() / 2], d[d.len() - 1], ancho * alto))
}

/// Resumen del plano para la IA.
pub fn resumen(p: &Plano, nombre: &str, bloque: Option<&str>) -> String {
    let mut t = format!(
        "Plano {nombre}: {} entidades en {} capas, {} bloques insertados, {} textos. Unidades: {}.\n",
        p.por_capa.values().sum::<usize>(),
        p.por_capa.len(),
        p.inserciones.len(),
        p.textos.len(),
        match p.unidades {
            4 => "mm".to_owned(),
            5 => "cm".to_owned(),
            6 => "m".to_owned(),
            0 => format!("sin indicar (se supone {})", if p.escala() < 1.0 { "mm" } else { "m" }),
            u => format!("código {u}"),
        }
    );
    // Bloques por nombre y capa
    let mut cuenta: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for i in &p.inserciones {
        *cuenta.entry((i.bloque.as_str(), i.capa.as_str())).or_default() += 1;
    }
    let mut v: Vec<_> = cuenta.into_iter().collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.1));
    t.push_str("\n| Bloque | Capa | Uds |\n|---|---|---:|\n");
    for ((b, c), n) in v.iter().take(80) {
        let _ = writeln!(t, "| {b} | {c} | {n} |");
    }
    if v.len() > 80 {
        let _ = writeln!(t, "(y {} combinaciones más)", v.len() - 80);
    }
    // Textos (sin repetir), para las leyendas
    let mut vistos = std::collections::BTreeSet::new();
    let textos: Vec<&(String, String)> = p.textos.iter().filter(|(x, _)| vistos.insert(x.clone())).collect();
    if !textos.is_empty() {
        t.push_str("\nTextos (sin repetir, para leyendas y rótulos):\n");
        for (x, c) in textos.iter().take(150) {
            let _ = writeln!(t, "- {x} [{c}]");
        }
        if textos.len() > 150 {
            let _ = writeln!(t, "(y {} textos más)", textos.len() - 150);
        }
    }
    if let Some(b) = bloque {
        match distancias(p, b) {
            Some((n, min, med, max, sup)) => {
                let _ = writeln!(
                    t,
                    "\nBloque «{b}»: {n} uds. Distancia al más cercano: mín. {min:.1} m, mediana {med:.1} m, máx. {max:.1} m. \
                     Rectángulo que ocupan: {sup:.0} m² ({:.0} m² por ud; orientativo, incluye huecos).",
                    sup / n as f64
                );
            }
            None => {
                let _ = writeln!(t, "\nBloque «{b}»: menos de 2 inserciones, sin distancias.");
            }
        }
    } else {
        t.push_str(
            "\nPara distancias y cobertura de un tipo de aparato, repita con «bloque» = su nombre \
             (identifíquelo por la leyenda).",
        );
    }
    t
}

/// Programa de LibreDWG que convierte DWG a DXF: la variable
/// `PPTO_DWG2DXF`, junto al ejecutable del programa o en el PATH.
pub fn conversor_dwg() -> Option<std::path::PathBuf> {
    let nombre = if cfg!(windows) { "dwg2dxf.exe" } else { "dwg2dxf" };
    if let Some(r) = std::env::var_os("PPTO_DWG2DXF").map(std::path::PathBuf::from)
        && r.is_file()
    {
        return Some(r);
    }
    let junto = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(nombre)))
        .filter(|r| r.is_file());
    junto.or_else(|| {
        std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(nombre)).find(|r| r.is_file()))
    })
}

/// Cómo conseguir el conversor (para el aviso).
pub const SIN_CONVERSOR: &str = "Para leer DWG hace falta dwg2dxf.exe de LibreDWG (libre, GPL-3): descargue el \
paquete de Windows (win64) de https://github.com/LibreDWG/libredwg/releases y deje dwg2dxf.exe y sus DLL junto a \
ppto-gui.exe. Mientras, guarde el plano como DXF (GstarCAD/AutoCAD: Guardar como → DXF).";

/// DXF de un DWG: el `.dxf` que ya haya al lado (si es más nuevo que el
/// DWG) o uno nuevo convertido con LibreDWG en la misma carpeta.
pub fn dxf_de_dwg(dwg: &Path) -> Result<std::path::PathBuf, String> {
    let dxf = dwg.with_extension("dxf");
    let fecha = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if dxf.is_file() && fecha(&dxf) >= fecha(dwg) {
        return Ok(dxf);
    }
    let conv = conversor_dwg().ok_or_else(|| SIN_CONVERSOR.to_owned())?;
    let mut hijo = std::process::Command::new(&conv)
        .arg("-y")
        .arg("-o")
        .arg(&dxf)
        .arg(dwg)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar {}: {e}", conv.display()))?;
    // Como mucho 3 minutos (planos muy grandes)
    let inicio = std::time::Instant::now();
    loop {
        match hijo.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if inicio.elapsed() > std::time::Duration::from_secs(180) => {
                let _ = hijo.kill();
                return Err("la conversión del DWG tarda demasiado: guárdelo como DXF desde el CAD".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    }
    if dxf.is_file() && std::fs::metadata(&dxf).is_ok_and(|m| m.len() > 0) {
        Ok(dxf)
    } else {
        Err("LibreDWG no ha podido convertir este DWG: guárdelo como DXF desde el CAD".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DXF sintético: 4 detectores en cuadrícula de 8 m (mm), un extintor,
    /// una leyenda en MTEXT.
    fn dxf() -> String {
        let mut s = String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
        for (x, y) in [(0, 0), (8000, 0), (0, 8000), (8000, 8000)] {
            s += &format!("0\nINSERT\n8\nPCI-DETECCION\n2\nDET_OPT\n10\n{x}\n20\n{y}\n30\n0\n");
        }
        s += "0\nINSERT\n8\nPCI-EXTINCION\n2\nEXT_6KG\n10\n100\n20\n100\n30\n0\n";
        s += "0\nMTEXT\n8\nLEYENDA\n1\n{\\fArial|b0;Detector \\Póptico}\n";
        s += "0\nTEXT\n8\nLEYENDA\n1\nExtintor 6 kg\n";
        s += "0\nLINE\n8\nMUROS\n10\n0\n20\n0\n11\n1\n21\n1\n";
        s += "0\nENDSEC\n0\nEOF\n";
        s
    }

    #[test]
    fn lee_bloques_textos_y_distancias() {
        let p = leer_texto(&dxf()).unwrap();
        assert_eq!(p.unidades, 4);
        assert_eq!(p.inserciones.len(), 5);
        assert_eq!(p.por_capa["MUROS"], 1);
        assert!(p.textos.iter().any(|(t, _)| t == "Detector óptico"), "{:?}", p.textos);
        let (n, min, med, max, sup) = distancias(&p, "det_opt").unwrap();
        assert_eq!(n, 4);
        assert!((min - 8.0).abs() < 1e-9 && (med - 8.0).abs() < 1e-9 && (max - 8.0).abs() < 1e-9);
        assert!((sup - 64.0).abs() < 1e-6);
        let t = resumen(&p, "plano.dxf", Some("DET_OPT"));
        assert!(t.contains("| DET_OPT | PCI-DETECCION | 4 |"), "{t}");
        assert!(t.contains("mediana 8.0 m"), "{t}");
        assert!(leer_texto("hola").is_err());
    }

    #[test]
    fn dwg_con_su_dxf_al_lado() {
        let dir = std::env::temp_dir().join(format!("dwg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dwg = dir.join("planta.dwg");
        std::fs::write(&dwg, b"AC1032").unwrap();
        // Sin DXF ni conversor (en las pruebas no hay LibreDWG): explica cómo
        if conversor_dwg().is_none() {
            let e = dxf_de_dwg(&dwg).unwrap_err();
            assert!(e.contains("LibreDWG") && e.contains("DXF"), "{e}");
        }
        // Con un DXF más nuevo al lado, se usa ese
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("planta.dxf"), dxf()).unwrap();
        assert_eq!(dxf_de_dwg(&dwg).unwrap(), dir.join("planta.dxf"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mtext_sin_formato() {
        assert_eq!(
            limpiar_mtext("{\\H2.5;\\C1;BIE 25 mm}\\Pcon armario"),
            "BIE 25 mm con armario"
        );
    }
}
