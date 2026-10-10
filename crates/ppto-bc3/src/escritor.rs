// SPDX-License-Identifier: GPL-3.0-or-later
//! Escritor FIEBDC-3 (P-011): «escritura estricta».
//!
//! Formato elegido para máxima compatibilidad con Presto 8.8, copiado de una
//! exportación real suya: `FIEBDC-3/2002`, juego de caracteres ANSI
//! (Windows-1252), fin de línea CRLF y `~K` con los decimales en el formato
//! antiguo. Ver `docs/bc3/matriz-registros.md`.

use ppto_core::concepto::{Concepto, Naturaleza};
use ppto_core::medicion::TipoLinea;
use ppto_core::{Decimal, ErrorMotor, Presupuesto};
use std::fmt::Write as _;
use std::path::Path;

/// Datos opcionales del fichero exportado.
#[derive(Debug, Clone, Default)]
pub struct OpcionesExportacion {
    /// Gastos generales, beneficio industrial e IVA para el `~K` (en puntos).
    /// Si faltan se escribe solo el porcentaje de costes indirectos, como Presto.
    pub gastos_generales: Option<Decimal>,
    pub beneficio_industrial: Option<Decimal>,
    pub iva: Option<Decimal>,
    /// Fecha de los precios en formato `DDMMAA` (vacía si `None`).
    pub fecha: Option<String>,
}

/// Fichero BC3 generado y avisos sobre datos que no caben en el formato.
#[derive(Debug, Clone)]
pub struct Exportacion {
    pub bytes: Vec<u8>,
    pub avisos: Vec<String>,
}

pub fn exportar(p: &Presupuesto, o: &OpcionesExportacion) -> Result<Exportacion, ErrorMotor> {
    let mut e = Escritor::default();
    e.escribir(p, o)?;
    let bytes = e.codificar();
    Ok(Exportacion {
        bytes,
        avisos: e.avisos,
    })
}

pub fn exportar_fichero(
    p: &Presupuesto,
    o: &OpcionesExportacion,
    ruta: impl AsRef<Path>,
) -> Result<Vec<String>, String> {
    let ex = exportar(p, o).map_err(|e| e.to_string())?;
    std::fs::write(ruta, &ex.bytes).map_err(|e| format!("no se pudo escribir el fichero: {e}"))?;
    Ok(ex.avisos)
}

#[derive(Default)]
struct Escritor {
    salida: String,
    avisos: Vec<String>,
    sustituidos: usize,
}

impl Escritor {
    fn registro(&mut self, r: &str) {
        self.salida.push_str(r);
        self.salida.push_str("\r\n");
    }

    fn escribir(&mut self, p: &Presupuesto, o: &OpcionesExportacion) -> Result<(), ErrorMotor> {
        let v = p.valorar()?;
        let d = p.decimales;
        let fecha = o.fecha.clone().unwrap_or_default();

        // ~V y ~K
        let rotulo = self.texto(&p.nombre);
        self.registro(&format!(
            "~V|presupuestos-bc3|FIEBDC-3/2002|presupuestos-bc3 {}|{rotulo}|ANSI|",
            env!("CARGO_PKG_VERSION")
        ));
        let mut k2 = num(p.costes_indirectos);
        if o.gastos_generales.is_some() || o.beneficio_industrial.is_some() || o.iva.is_some() {
            let s = |x: Option<Decimal>| x.map(num).unwrap_or_default();
            let _ = write!(
                k2,
                "\\{}\\{}\\0\\{}",
                s(o.gastos_generales),
                s(o.beneficio_industrial),
                s(o.iva)
            );
        }
        self.registro(&format!(
            "~K|\\{}\\{}\\{}\\{}\\{}\\{}\\{}\\EUR\\|{}|",
            d.dimensiones, d.medicion, d.rendimiento, d.importe_linea, d.precio, d.importe, d.importe, k2
        ));

        // Precio declarado de cada concepto: el de venta (con indirectos) si cuelga
        // de un capítulo, el coste si es auxiliar o recurso; total si es capítulo.
        let mut declarado = v.precios.clone();
        for (padre, lineas) in &v.lineas {
            if p.conceptos[padre].naturaleza == Naturaleza::Capitulo {
                for l in lineas {
                    if p.conceptos[&l.hijo].naturaleza != Naturaleza::Capitulo {
                        declarado.insert(l.hijo.clone(), l.precio);
                    }
                }
            }
        }

        // ~C ~D ~T: raíz primero, luego capítulos y el resto, en orden estable
        let mut orden: Vec<&Concepto> = p.conceptos.values().collect();
        orden.sort_by_key(|c| {
            (
                c.codigo != p.raiz,
                c.naturaleza != Naturaleza::Capitulo,
                c.descomposicion.is_empty(),
                c.codigo.clone(),
            )
        });
        let mut subcontratas = 0;
        for c in orden {
            let codigo = self.codigo_con_marca(p, c);
            let (tipo, precio) = match c.naturaleza {
                Naturaleza::ManoObra => ("1", Some(c.precio)),
                Naturaleza::Maquinaria => ("2", Some(c.precio)),
                Naturaleza::Material => ("3", Some(c.precio)),
                Naturaleza::Porcentaje => ("0", None),
                Naturaleza::Subcontrata => {
                    subcontratas += 1;
                    ("0", Some(c.precio))
                }
                _ => ("0", declarado.get(&c.codigo).copied()),
            };
            let unidad = self.texto(&c.unidad);
            let resumen = self.texto(&c.resumen);
            self.registro(&format!(
                "~C|{codigo}|{unidad}|{resumen}|{}|{fecha}|{tipo}|",
                precio.map(num).unwrap_or_default()
            ));
            if !c.descomposicion.is_empty() {
                let mut lineas = String::new();
                for l in &c.descomposicion {
                    let hijo = &p.conceptos[&l.hijo];
                    let (factor, rend) = if hijo.naturaleza == Naturaleza::Porcentaje {
                        // FIEBDC-3: el porcentaje viaja en fracción (3 % → 0.03)
                        (Decimal::ONE, l.cantidad() / Decimal::ONE_HUNDRED)
                    } else {
                        (l.factor, l.rendimiento)
                    };
                    let _ = write!(lineas, "{}\\{}\\{}\\", self.texto(&l.hijo), num(factor), num(rend));
                }
                self.registro(&format!("~D|{codigo}|{lineas}|"));
            }
            if let Some(t) = c.texto.as_deref().filter(|t| !t.trim().is_empty()) {
                let t = self.texto_largo(t);
                let cod = self.texto(&c.codigo);
                self.registro(&format!("~T|{cod}|{t}|"));
            }
        }
        if subcontratas > 0 {
            self.avisos.push(format!(
                "{subcontratas} subcontrata(s) exportada(s) con tipo 0: FIEBDC-3/2002 no tiene tipo de subcontrata"
            ));
        }

        // ~M
        for ((padre, hijo), m) in &p.mediciones {
            let total = p
                .conceptos
                .get(padre)
                .and_then(|c| c.descomposicion.iter().find(|l| &l.hijo == hijo))
                .map(|l| l.cantidad())
                .unwrap_or_default();
            let padre_c = self.codigo_con_marca(p, &p.conceptos[padre]);
            let mut lineas = String::new();
            for l in &m.lineas {
                let (tipo, comentario) = match l.tipo {
                    TipoLinea::Normal => ("", l.comentario.clone()),
                    TipoLinea::SubtotalParcial => ("1", l.comentario.clone()),
                    TipoLinea::SubtotalAcumulado => ("2", l.comentario.clone()),
                    TipoLinea::Formula => ("3", l.formula.clone().unwrap_or_default()),
                };
                let s = |x: Option<Decimal>| x.map(num).unwrap_or_default();
                let comentario = self.texto(&comentario);
                let _ = write!(
                    lineas,
                    "{tipo}\\{comentario}\\{}\\{}\\{}\\{}\\",
                    s(l.unidades),
                    s(l.longitud),
                    s(l.anchura),
                    s(l.altura)
                );
            }
            let hijo_c = self.texto(hijo);
            self.registro(&format!("~M|{padre_c}\\{hijo_c}||{}|{lineas}|", num(total)));
        }
        if self.sustituidos > 0 {
            self.avisos.push(format!(
                "{} carácter(es) sin equivalente en ANSI o reservados del formato (| ~ \\) se han sustituido",
                self.sustituidos
            ));
        }
        Ok(())
    }

    fn codigo_con_marca(&mut self, p: &Presupuesto, c: &Concepto) -> String {
        let base = self.texto(&c.codigo);
        if c.codigo == p.raiz {
            format!("{base}##")
        } else if c.naturaleza == Naturaleza::Capitulo {
            format!("{base}#")
        } else {
            base
        }
    }

    /// Campo de una línea: sin separadores del formato ni saltos de línea.
    fn texto(&mut self, s: &str) -> String {
        s.chars()
            .map(|ch| match ch {
                '|' | '\\' => {
                    self.sustituidos += 1;
                    '/'
                }
                '~' => {
                    self.sustituidos += 1;
                    '-'
                }
                '\r' | '\n' | '\t' => ' ',
                c => c,
            })
            .collect()
    }

    /// Texto largo: admite saltos de línea (CRLF), no separadores.
    fn texto_largo(&mut self, s: &str) -> String {
        let normal = s.replace("\r\n", "\n");
        normal
            .split('\n')
            .map(|l| self.texto(l))
            .collect::<Vec<_>>()
            .join("\r\n")
    }

    /// Codifica en Windows-1252 sustituyendo por «?» lo que no tenga equivalente.
    fn codificar(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.salida.len());
        let mut buf = [0u8; 4];
        let mut fuera = 0;
        for ch in self.salida.chars() {
            let (b, _, error) = encoding_rs::WINDOWS_1252.encode(ch.encode_utf8(&mut buf));
            if error {
                fuera += 1;
                out.push(b'?');
            } else {
                out.extend_from_slice(&b);
            }
        }
        if fuera > 0 {
            self.avisos.push(format!(
                "{fuera} carácter(es) sin equivalente en ANSI (Windows-1252) se han sustituido por «?»"
            ));
        }
        out
    }
}

/// Número con punto decimal y sin ceros sobrantes (24.50 → 24.5).
fn num(v: Decimal) -> String {
    let s = v.normalize().to_string();
    if s == "-0" { "0".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn numeros() {
        assert_eq!(num(dec!(24.50)), "24.5");
        assert_eq!(num(dec!(210.00)), "210");
        assert_eq!(num(dec!(0.020)), "0.02");
        assert_eq!(num(dec!(-1)), "-1");
    }

    #[test]
    fn campos_saneados() {
        let mut e = Escritor::default();
        assert_eq!(e.texto("a|b\\c~d\ne"), "a/b/c-d e");
        assert_eq!(e.sustituidos, 3);
        assert_eq!(e.texto_largo("uno\r\ndos|"), "uno\r\ndos/");
    }
}
