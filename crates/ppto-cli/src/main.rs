// SPDX-License-Identifier: GPL-3.0-or-later
//! `ppto` — demostración reproducible del motor económico.
//!
//! Uso:
//!   ppto demo                 Imprime el caso sintético de suelo radiante.
//!   ppto demo --db <fichero>  Además lo guarda en SQLite como revisión R1.

use ppto_core::concepto::Naturaleza;
use ppto_core::ejemplos::{ofertas_montaje_suelo_radiante, suelo_radiante};
use ppto_core::subcontrata::simular;
use ppto_core::{Decimal, Presupuesto, redondear, venta};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("demo") => {
            let db = args.iter().position(|a| a == "--db").and_then(|i| args.get(i + 1));
            match demo(db.map(String::as_str)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("uso: ppto demo [--db <fichero.sqlite>]");
            ExitCode::from(2)
        }
    }
}

/// Formato español con separador de miles: 7.005,15
fn num(v: Decimal, d: u32) -> String {
    let s = format!("{:.*}", d as usize, redondear(v, d));
    let (ent, dec) = s.split_once('.').map_or((s.as_str(), None), |(e, f)| (e, Some(f)));
    let (signo, dig) = ent.strip_prefix('-').map_or(("", ent), |x| ("-", x));
    let mut out = String::from(signo);
    for (i, c) in dig.chars().enumerate() {
        if i > 0 && (dig.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    if let Some(f) = dec {
        out.push(',');
        out.push_str(f);
    }
    out
}

fn eur(v: Decimal) -> String {
    num(v, 2)
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
            eur(r.pem_escenario),
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formato_espanol() {
        assert_eq!(eur(dec!(7005.15)), "7.005,15");
        assert_eq!(eur(dec!(10086.72)), "10.086,72");
        assert_eq!(eur(dec!(-2.1)), "-2,10");
        assert_eq!(eur(dec!(123)), "123,00");
        assert_eq!(eur(dec!(1234567.891)), "1.234.567,89");
        assert_eq!(num(dec!(1470), 3), "1.470,000");
        assert_eq!(num(dec!(0.125), 2), "0,13");
    }
}
