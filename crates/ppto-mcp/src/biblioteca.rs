// SPDX-License-Identifier: GPL-3.0-or-later
//! Herramientas de la biblioteca de precios del usuario (partidas guardadas
//! de obras anteriores, tarifas de fabricante por año, descuentos y ofertas).

use crate::{Estado, dec_campo, texto};
use ppto_biblioteca::{Biblioteca, LineaOferta, LineaPvp, Oferta, Origen, anio, hoy};
use ppto_core::formato::{eur, num};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;

fn ruta(estado: &Estado) -> Result<std::path::PathBuf, String> {
    estado
        .ruta_biblioteca
        .clone()
        .ok_or_else(|| "no se encuentra la carpeta de configuración".to_owned())
}

fn cadena(v: &Value, campo: &str) -> String {
    v.get(campo).and_then(Value::as_str).unwrap_or("").trim().to_owned()
}

pub(crate) fn buscar(estado: &Estado, a: &Value) -> Result<String, String> {
    let q = texto(a, "texto")?;
    let limite = usize::try_from(a.get("limite").and_then(Value::as_u64).unwrap_or(10)).unwrap_or(10);
    let b = Biblioteca::cargar(&ruta(estado)?);
    let actual = anio(&hoy());
    let mut t = String::new();
    let partidas = b.buscar(q, limite);
    if partidas.is_empty() {
        let _ = writeln!(t, "Ninguna partida de la biblioteca contiene «{q}».");
    }
    for (codigo, v) in &partidas {
        let _ = writeln!(
            t,
            "\n## {codigo} · {} · {}\nGuardada {} (obra «{}», {}), precio entonces {} €.",
            v.unidad,
            v.resumen,
            v.fecha,
            v.obra,
            v.origen.texto(),
            eur(v.precio)
        );
        let _ = writeln!(
            t,
            "| Código | Nat. | Ud | Resumen | Rend. | Precio hoy € | Origen del precio |\n|---|---|---|---|---:|---:|---|"
        );
        for l in &v.lineas {
            let (neto, _, nota) = b.precio_hoy(l, actual);
            let _ = writeln!(
                t,
                "| {} | {} | {} | {} | {} | {} | {} |",
                l.codigo,
                crate::nombre_naturaleza(l.naturaleza),
                l.unidad,
                l.resumen,
                num(l.rendimiento, 4),
                if l.naturaleza == ppto_core::Naturaleza::Porcentaje {
                    "%".into()
                } else {
                    num(neto, 2)
                },
                nota.unwrap_or_else(|| "guardado".into())
            );
        }
    }
    let precios = b.buscar_precios(q, 20);
    if !precios.is_empty() {
        t.push_str("\n## Tarifas y ofertas\n| Fabricante | Fuente | Referencia | Descripción | € | Tipo |\n|---|---|---|---|---:|---|\n");
        for (f, fuente, r, d, p, neto) in &precios {
            let _ = writeln!(
                t,
                "| {f} | {fuente} | {r} | {d} | {} | {} |",
                num(*p, 2),
                if *neto { "neto" } else { "PVP" }
            );
        }
    }
    let antiguas = b.tarifas_antiguas(actual);
    if !antiguas.is_empty() {
        t.push_str("\nTarifas de años anteriores (avise al usuario si las usa): ");
        t.push_str(
            &antiguas
                .iter()
                .map(|(f, e, _)| format!("{f} {e}"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        t.push('\n');
    }
    t.push_str(
        "\nUse estas partidas como base (confianza alta si son reales o revisadas y el precio es de hoy) \
         y cite la obra de origen en la justificación.",
    );
    Ok(t)
}

pub(crate) fn estado_biblioteca(estado: &Estado) -> Result<String, String> {
    let b = Biblioteca::cargar(&ruta(estado)?);
    let actual = anio(&hoy());
    let mut t = format!("Biblioteca: {} partidas guardadas.\n\n", b.partidas.len());
    if b.fabricantes.is_empty() {
        t.push_str("Sin fabricantes (ni tarifas, descuentos u ofertas) todavía.");
        return Ok(t);
    }
    t.push_str("| Fabricante | Tarifa vigente | Refs | Descuentos | Ofertas | Subida estimada |\n|---|---|---:|---|---:|---|\n");
    for f in b.fabricantes.values() {
        let tv = f.vigente().map_or("—".to_owned(), |v| {
            if v.anio < actual {
                format!("{} (ANTIGUA)", v.etiqueta)
            } else {
                v.etiqueta.clone()
            }
        });
        let dtos = f
            .descuentos
            .iter()
            .map(|(g, p)| {
                if g.is_empty() {
                    format!("{} %", num(*p, 2))
                } else {
                    format!("{g} {} %", num(*p, 2))
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            t,
            "| {} | {tv} | {} | {dtos} | {} | {} |",
            f.nombre,
            f.vigente().map_or(0, |v| v.lineas.len()),
            f.ofertas.len(),
            f.subida_estimada.map_or("—".into(), |s| format!("{} %", num(s, 2)))
        );
    }
    Ok(t)
}

pub(crate) fn registrar_tarifa(estado: &Estado, a: &Value) -> Result<String, String> {
    let fab = texto(a, "fabricante")?.to_owned();
    let etiqueta = texto(a, "etiqueta")?.to_owned();
    let an = a
        .get("anio")
        .and_then(Value::as_i64)
        .and_then(|x| i32::try_from(x).ok())
        .unwrap_or_else(|| anio(&etiqueta));
    if an < 2000 {
        return Err("indique el año de la tarifa (anio)".into());
    }
    let mut lineas = BTreeMap::new();
    for l in a.get("lineas").and_then(Value::as_array).ok_or("indique lineas")? {
        let r = texto(l, "referencia")?.to_owned();
        let pvp = dec_campo(l, "pvp")?.ok_or_else(|| format!("{r}: sin pvp"))?;
        lineas.insert(
            r,
            LineaPvp {
                descripcion: cadena(l, "descripcion"),
                unidad: cadena(l, "unidad"),
                pvp,
                grupo: Some(cadena(l, "grupo")).filter(|g| !g.is_empty()),
            },
        );
    }
    let cmp = Biblioteca::modificar(&ruta(estado)?, |b| b.cargar_tarifa(&fab, &etiqueta, an, lineas))
        .map_err(|e| e.to_string())?;
    let mut t = format!(
        "Tarifa {fab} {etiqueta} guardada en la biblioteca: {} referencias.",
        cmp.lineas
    );
    if let Some(m) = cmp.media {
        let _ = write!(
            t,
            "\nFrente a la anterior: variación media {} %, {} nuevas, {} desaparecidas.",
            num(m, 2),
            cmp.nuevas.len(),
            cmp.desaparecidas.len()
        );
        for (r, x, y, p) in cmp.mayores.iter().take(5) {
            let _ = write!(t, "\n- {r}: {} → {} € ({} %)", num(*x, 2), num(*y, 2), num(*p, 2));
        }
    }
    Ok(t)
}

pub(crate) fn registrar_oferta(estado: &Estado, a: &Value) -> Result<String, String> {
    let fab = texto(a, "fabricante")?.to_owned();
    let mut lineas = Vec::new();
    for l in a.get("lineas").and_then(Value::as_array).ok_or("indique lineas")? {
        let r = texto(l, "referencia")?.to_owned();
        lineas.push(LineaOferta {
            neto: dec_campo(l, "neto")?.ok_or_else(|| format!("{r}: sin neto"))?,
            referencia: r,
            descripcion: cadena(l, "descripcion"),
            unidad: cadena(l, "unidad"),
            cantidad: dec_campo(l, "cantidad")?,
        });
    }
    let oferta = Oferta {
        numero: cadena(a, "numero"),
        fecha: Some(cadena(a, "fecha")).filter(|f| !f.is_empty()).unwrap_or_else(hoy),
        validez: Some(cadena(a, "validez")).filter(|v| !v.is_empty()),
        obra: Some(cadena(a, "obra"))
            .filter(|o| !o.is_empty())
            .or_else(|| estado.obra_activa.clone())
            .unwrap_or_default(),
        lineas,
    };
    let numero = oferta.numero.clone();
    let n = oferta.lineas.len();
    let dtos = Biblioteca::modificar(&ruta(estado)?, |b| {
        b.anadir_oferta(&fab, oferta);
        b.descuento_de_oferta(&fab, &numero)
    })
    .map_err(|e| e.to_string())?;
    let mut t = format!("Oferta {fab} {numero} guardada en la biblioteca ({n} líneas en netos).");
    if !dtos.is_empty() {
        t.push_str("\nDescuento real frente a la tarifa de la biblioteca:");
        for (g, (d, k)) in &dtos {
            let g = if g.is_empty() { "general" } else { g };
            let _ = write!(t, "\n- {g}: {} % ({k} líneas)", num(*d, 2));
        }
    }
    Ok(t)
}

pub(crate) fn fijar_descuento(estado: &Estado, a: &Value) -> Result<String, String> {
    let fab = texto(a, "fabricante")?.to_owned();
    let pct = dec_campo(a, "porcentaje")?;
    let subida = dec_campo(a, "subida_estimada")?;
    if pct.is_none() && subida.is_none() {
        return Err("indique porcentaje y/o subida_estimada".into());
    }
    Biblioteca::modificar(&ruta(estado)?, |b| {
        if let Some(p) = pct {
            b.fijar_descuento(&fab, Some(p));
        }
        if subida.is_some() {
            let (f, _) = ppto_core::descuentos::partes(&fab);
            b.fijar_subida(&f, subida);
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(format!("Biblioteca actualizada para {fab}."))
}

pub(crate) fn guardar_obra(estado: &Estado, a: &Value) -> Result<String, String> {
    let p = estado.p()?;
    let obra = estado.obra_activa.clone().unwrap_or_else(|| p.nombre.clone());
    let origen = match a.get("origen").and_then(Value::as_str).unwrap_or("revisado") {
        "real" => Origen::Real,
        "estimado" => Origen::Estimado,
        _ => Origen::Revisado,
    };
    let solo: Option<Vec<String>> = a
        .get("partidas")
        .and_then(Value::as_array)
        .map(|v| v.iter().filter_map(Value::as_str).map(str::to_owned).collect());
    let i = Biblioteca::modificar(&ruta(estado)?, |b| b.guardar_obra(p, &obra, origen, solo.as_deref()))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Guardadas en la biblioteca desde «{obra}»: {} nuevas, {} con versión nueva, {} sin cambios. Descuentos aprendidos: {}.",
        i.nuevas.len(),
        i.actualizadas.len(),
        i.iguales,
        if i.descuentos.is_empty() {
            "ninguno".into()
        } else {
            i.descuentos.join(", ")
        }
    ))
}
