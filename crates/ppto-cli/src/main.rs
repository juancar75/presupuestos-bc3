// SPDX-License-Identifier: GPL-3.0-or-later
//! `ppto` — demostración reproducible del motor económico.
//!
//! Uso:
//!   ppto demo                 Imprime el caso sintético de suelo radiante.
//!   ppto demo --db <fichero>  Además lo guarda en SQLite como revisión R1.
//!   ppto demo --excel <f.xlsx> Además genera el informe Excel.
//!   ppto demo --bc3 <f.bc3>   Además exporta el ejemplo a BC3.
//!   ppto importar <f.bc3> [--db <f.sqlite>] [--excel <f.xlsx>] [--bc3 <salida.bc3>] [--todo]
//!                             Importa un BC3, informa de incidencias y de los
//!                             precios que no cuadran; opcionalmente lo guarda.

use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::formato::{eur, num};
use ppto_core::subcontrata::simular;
use ppto_core::{Decimal, Presupuesto, venta};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("demo") => {
            let db = args.iter().position(|a| a == "--db").and_then(|i| args.get(i + 1));
            let xlsx = args.iter().position(|a| a == "--excel").and_then(|i| args.get(i + 1));
            let bc3 = args.iter().position(|a| a == "--bc3").and_then(|i| args.get(i + 1));
            match demo(db.map(String::as_str))
                .and_then(|()| match xlsx {
                    Some(x) => excel(&suelo_radiante(), &ppto_informes::OpcionesInforme::default(), x),
                    None => Ok(()),
                })
                .and_then(|()| match bc3 {
                    Some(b) => exportar_bc3(&suelo_radiante(), &ppto_bc3::OpcionesExportacion::default(), b),
                    None => Ok(()),
                }) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("importar") if args.len() >= 2 => {
            let db = args.iter().position(|a| a == "--db").and_then(|i| args.get(i + 1));
            let todo = args.iter().any(|a| a == "--todo");
            let xlsx = args.iter().position(|a| a == "--excel").and_then(|i| args.get(i + 1));
            let bc3 = args.iter().position(|a| a == "--bc3").and_then(|i| args.get(i + 1));
            match importar(
                &args[1],
                db.map(String::as_str),
                xlsx.map(String::as_str),
                bc3.map(String::as_str),
                todo,
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "uso:\n  ppto demo [--db <fichero.sqlite>] [--excel <fichero.xlsx>] [--bc3 <fichero.bc3>]\n  ppto importar <fichero.bc3> [--db <fichero.sqlite>] [--excel <fichero.xlsx>] [--bc3 <salida.bc3>] [--todo]"
            );
            ExitCode::from(2)
        }
    }
}

fn excel(p: &Presupuesto, o: &ppto_informes::OpcionesInforme, ruta: &str) -> Result<(), Box<dyn std::error::Error>> {
    ppto_informes::guardar_excel(p, o, ruta)?;
    println!("Informe Excel: {ruta}");
    Ok(())
}

fn exportar_bc3(
    p: &Presupuesto,
    o: &ppto_bc3::OpcionesExportacion,
    ruta: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let avisos = ppto_bc3::exportar_fichero(p, o, ruta)?;
    println!("BC3 exportado: {ruta}");
    for a in avisos {
        println!("  [aviso] {a}");
    }
    Ok(())
}

fn importar(
    ruta: &str,
    db: Option<&str>,
    xlsx: Option<&str>,
    bc3: Option<&str>,
    todo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let imp = ppto_bc3::importar_fichero(ruta)?;
    let p = &imp.presupuesto;
    let c = &imp.cabecera;
    println!("{ruta}");
    println!(
        "  Programa: {}  ·  Formato: {}  ·  Caracteres: {}",
        c.programa, c.version_formato, c.juego_caracteres
    );
    let regs: Vec<String> = imp.registros.iter().map(|(k, v)| format!("~{k} {v}")).collect();
    println!("  Registros: {}", regs.join("  "));
    println!(
        "  Conceptos: {}  ·  Mediciones: {}",
        p.conceptos.len(),
        p.mediciones.len()
    );
    let k = &imp.porcentajes;
    let pc = |v: Option<Decimal>| v.map(|x| format!("{} %", num(x, 2))).unwrap_or_else(|| "—".into());
    println!(
        "  ~K: CI {}  GG {}  BI {}  baja {}  IVA {}",
        pc(k.costes_indirectos),
        pc(k.gastos_generales),
        pc(k.beneficio_industrial),
        pc(k.baja),
        pc(k.iva)
    );

    let limite = if todo { usize::MAX } else { 25 };
    println!(
        "\nINCIDENCIAS: {} errores, {} avisos, {} en total",
        imp.errores(),
        imp.avisos(),
        imp.incidencias.len()
    );
    for i in imp.incidencias.iter().take(limite) {
        println!("  {i}");
    }
    if imp.incidencias.len() > limite {
        println!("  … {} más (use --todo)", imp.incidencias.len() - limite);
    }

    println!("\nPRECIOS QUE NO CUADRAN CON EL BC3: {}", imp.discrepancias.len());
    for d in imp.discrepancias.iter().take(limite) {
        println!(
            "  {:<14} declarado {:>12}  calculado {:>12}  diferencia {:>10}  {}",
            d.codigo,
            eur(d.declarado),
            eur(d.calculado),
            eur(d.diferencia()),
            d.resumen.chars().take(40).collect::<String>()
        );
    }
    if imp.discrepancias.len() > limite {
        println!("  … {} más (use --todo)", imp.discrepancias.len() - limite);
    }

    let pem = p.pem()?;
    if !p.costes_indirectos.is_zero() {
        println!(
            "\nCoste directo {} €  +  {} % costes indirectos por partida",
            eur(p.coste_directo()?),
            num(p.costes_indirectos, 2)
        );
    }
    match imp.pem_declarado {
        Some(d) if d == pem => println!("\nPEM {} € (coincide con el BC3)", eur(pem)),
        Some(d) => println!(
            "\nPEM calculado {} €  ·  declarado en el BC3 {} €  ·  diferencia {} €",
            eur(pem),
            eur(d),
            eur(pem - d)
        ),
        None => println!("\nPEM {} €", eur(pem)),
    }

    if let Some(ruta_db) = db {
        let mut a = ppto_db::Almacen::abrir(ruta_db)?;
        let id = a.crear_presupuesto(&p.nombre)?;
        let rev = a.guardar_revision(id, "R1", "importación BC3", p)?;
        println!("Guardado en {ruta_db} (presupuesto {id}, revisión {rev}).");
    }
    if let Some(x) = xlsx {
        let k = &imp.porcentajes;
        let d = ppto_informes::OpcionesInforme::default();
        let o = ppto_informes::OpcionesInforme {
            gastos_generales: k.gastos_generales.unwrap_or(d.gastos_generales),
            beneficio_industrial: k.beneficio_industrial.unwrap_or(d.beneficio_industrial),
            iva: k.iva.unwrap_or(d.iva),
        };
        excel(p, &o, x)?;
    }
    if let Some(b) = bc3 {
        let k = &imp.porcentajes;
        let o = ppto_bc3::OpcionesExportacion {
            gastos_generales: k.gastos_generales,
            beneficio_industrial: k.beneficio_industrial,
            iva: k.iva,
            fecha: None,
        };
        exportar_bc3(p, &o, b)?;
    }
    Ok(())
}

fn demo(db: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let p = suelo_radiante();
    println!("{}\n", p.nombre);
    descompuesto(&p, "SR.M2")?;
    descompuesto(&p, "SR.COL8")?;

    println!("CAPÍTULO C01 {}", p.concepto("C01")?.resumen);
    for l in p.lineas_valoradas("C01")? {
        let c = p.concepto(&l.hijo)?;
        println!(
            "  {:<8} {:<4} {:>10} × {:>9} = {:>11}",
            l.hijo,
            c.unidad,
            num(l.cantidad, 2),
            eur(l.precio),
            eur(l.importe)
        );
    }
    let pem = p.pem()?;
    println!("  PEM{:>45}\n", eur(pem));

    let e = p.explosion_recursos()?;
    println!("RECURSOS");
    for r in &e.recursos {
        let (cant, precio) = if r.naturaleza == Naturaleza::Porcentaje {
            (String::new(), String::new())
        } else {
            (num(r.cantidad, 3), eur(r.precio))
        };
        println!(
            "  {:<10} {:<3} {:>10}   {:>7}   {:>10}  {}",
            r.codigo,
            r.unidad,
            cant,
            precio,
            eur(r.importe),
            r.resumen
        );
    }
    let horas: Decimal = e.por_naturaleza(Naturaleza::ManoObra).map(|r| r.cantidad).sum();
    println!("  Horas de mano de obra: {}", num(horas, 2));
    println!(
        "  Total explosión {}  ·  descuadre de redondeo frente al PEM: {}\n",
        eur(e.total),
        eur(e.descuadre_redondeo)
    );

    println!("ESCENARIOS DE SUBCONTRATACIÓN DEL MONTAJE");
    for paq in ofertas_montaje_suelo_radiante() {
        let r = simular(&p, std::slice::from_ref(&paq))?;
        let eq = r.precio_equivalente(&paq.codigo, "SR.M2", 2).unwrap_or_default();
        let h: Decimal = r.horas_liberadas.values().copied().sum();
        println!(
            "  {} {:<38} contratado {:>9} ({:>6} €/m²)  PEM {:>9}  ahorro {:>7}  horas liberadas {}",
            paq.codigo,
            paq.descripcion,
            eur(r.coste_contratado),
            eur(eq),
            eur(r.coste_escenario),
            eur(r.ahorro),
            num(h, 1)
        );
        for n in &r.notas {
            println!("      nota: {n}");
        }
    }

    let rs = venta::resumen(pem, venta::GG_HABITUAL, venta::BI_HABITUAL, venta::IVA_GENERAL, 2);
    println!("\nRESUMEN");
    println!("  PEM                      {:>11}", eur(rs.pem));
    println!("  13 % Gastos generales    {:>11}", eur(rs.gastos_generales));
    println!("   6 % Beneficio industr.  {:>11}", eur(rs.beneficio_industrial));
    println!("  Base                     {:>11}", eur(rs.base));
    println!("  21 % IVA                 {:>11}", eur(rs.iva));
    println!("  TOTAL                    {:>11}", eur(rs.total));

    if let Some(ruta) = db {
        let mut a = ppto_db::Almacen::abrir(ruta)?;
        let id = a.crear_presupuesto(&p.nombre)?;
        let rev = a.guardar_revision(id, "R1", "demo", &p)?;
        println!("\nGuardado en {ruta} (presupuesto {id}, revisión {rev}).");
    }
    Ok(())
}

fn descompuesto(p: &Presupuesto, codigo: &str) -> Result<(), ppto_core::ErrorMotor> {
    let c = p.concepto(codigo)?;
    println!("{} {} {}", c.codigo, c.unidad, c.resumen);
    for l in p.lineas_valoradas(codigo)? {
        let h = p.concepto(&l.hijo)?;
        println!(
            "  {:<10} {:<3} {:>7} × {:>7} = {:>7}  {}",
            l.hijo,
            h.unidad,
            num(l.cantidad, 3),
            eur(l.precio),
            eur(l.importe),
            h.resumen
        );
    }
    println!("  Precio{:>36}\n", eur(p.precio(codigo)?));
    Ok(())
}
