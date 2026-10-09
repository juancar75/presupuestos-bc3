// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-db
//!
//! Persistencia SQLite del gestor de presupuestos:
//! - Migraciones versionadas con `PRAGMA user_version`.
//! - Revisiones como copias completas e independientes (P-009).
//! - Revisiones bloqueables (p. ej. la ofertada al cliente).
//! - Toda escritura en una transacción y registrada en `auditoria`.

use ppto_core::medicion::{LineaMedicion, Medicion, TipoLinea};
use ppto_core::presupuesto::OpcionesCi;
use ppto_core::{Concepto, Decimal, Decimales, LineaDescomposicion, Naturaleza, Presupuesto};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum ErrorDb {
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("dato corrupto en la base de datos: {0}")]
    Corrupto(String),
    #[error("la revisión {0} está bloqueada y no admite cambios")]
    RevisionBloqueada(i64),
    #[error("la revisión {0} no existe")]
    RevisionInexistente(i64),
    #[error("motor: {0}")]
    Motor(#[from] ppto_core::ErrorMotor),
}

pub type Resultado<T> = Result<T, ErrorDb>;

/// Datos de cabecera de una revisión guardada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoRevision {
    pub id: i64,
    pub presupuesto_id: i64,
    pub presupuesto: String,
    pub etiqueta: String,
    pub autor: String,
    pub creado_en: String,
    pub bloqueada: bool,
}

const MIGRACIONES: &[&str] = &[
    include_str!("../migrations/0001_esquema_inicial.sql"),
    include_str!("../migrations/0002_costes_indirectos.sql"),
];

pub struct Almacen {
    con: Connection,
}

impl Almacen {
    pub fn abrir(ruta: impl AsRef<Path>) -> Resultado<Self> {
        Self::preparar(Connection::open(ruta)?)
    }

    pub fn en_memoria() -> Resultado<Self> {
        Self::preparar(Connection::open_in_memory()?)
    }

    fn preparar(con: Connection) -> Resultado<Self> {
        con.pragma_update(None, "foreign_keys", "ON")?;
        let mut a = Self { con };
        a.migrar()?;
        Ok(a)
    }

    pub fn version_esquema(&self) -> Resultado<usize> {
        Ok(self
            .con
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize)
    }

    fn migrar(&mut self) -> Resultado<()> {
        let actual = self.version_esquema()?;
        for (i, sql) in MIGRACIONES.iter().enumerate().skip(actual) {
            let tx = self.con.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn crear_presupuesto(&mut self, nombre: &str) -> Resultado<i64> {
        self.con
            .execute("INSERT INTO presupuestos (nombre) VALUES (?1)", [nombre])?;
        Ok(self.con.last_insert_rowid())
    }

    /// Guarda `p` como una revisión nueva del presupuesto `presupuesto_id`.
    pub fn guardar_revision(
        &mut self,
        presupuesto_id: i64,
        etiqueta: &str,
        autor: &str,
        p: &Presupuesto,
    ) -> Resultado<i64> {
        p.validar()?;
        let tx = self.con.transaction()?;
        let d = p.decimales;
        tx.execute(
            "INSERT INTO revisiones (presupuesto_id, etiqueta, autor, nombre, raiz,
                dec_dimensiones, dec_medicion, dec_rendimiento, dec_importe_linea, dec_precio, dec_importe,
                costes_indirectos, ci_redondear_coste_antes, ci_aplicar_sin_descomponer)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                presupuesto_id,
                etiqueta,
                autor,
                p.nombre,
                p.raiz,
                d.dimensiones,
                d.medicion,
                d.rendimiento,
                d.importe_linea,
                d.precio,
                d.importe,
                p.costes_indirectos.to_string(),
                p.opciones_ci.redondear_coste_antes,
                p.opciones_ci.aplicar_a_sin_descomponer
            ],
        )?;
        let rev = tx.last_insert_rowid();
        escribir_contenido(&tx, rev, p)?;
        auditar(&tx, rev, autor, "guardar_revision", &format!("etiqueta={etiqueta}"))?;
        tx.commit()?;
        Ok(rev)
    }

    /// Crea una revisión nueva copiando íntegramente otra.
    pub fn derivar_revision(&mut self, origen: i64, etiqueta: &str, autor: &str) -> Resultado<i64> {
        let tx = self.con.transaction()?;
        let n = tx.execute(
            "INSERT INTO revisiones (presupuesto_id, etiqueta, revision_origen_id, autor, nombre, raiz,
                dec_dimensiones, dec_medicion, dec_rendimiento, dec_importe_linea, dec_precio, dec_importe,
                costes_indirectos, ci_redondear_coste_antes, ci_aplicar_sin_descomponer)
             SELECT presupuesto_id, ?2, id, ?3, nombre, raiz,
                dec_dimensiones, dec_medicion, dec_rendimiento, dec_importe_linea, dec_precio, dec_importe,
                costes_indirectos, ci_redondear_coste_antes, ci_aplicar_sin_descomponer
             FROM revisiones WHERE id = ?1",
            params![origen, etiqueta, autor],
        )?;
        if n == 0 {
            return Err(ErrorDb::RevisionInexistente(origen));
        }
        let nueva = tx.last_insert_rowid();
        for tabla in ["conceptos", "descomposicion", "medicion_lineas"] {
            let cols = columnas(&tx, tabla)?;
            let resto: Vec<&str> = cols
                .iter()
                .map(String::as_str)
                .filter(|c| *c != "revision_id")
                .collect();
            tx.execute(
                &format!(
                    "INSERT INTO {tabla} (revision_id, {c}) SELECT ?1, {c} FROM {tabla} WHERE revision_id = ?2",
                    c = resto.join(", ")
                ),
                params![nueva, origen],
            )?;
        }
        auditar(&tx, nueva, autor, "derivar_revision", &format!("origen={origen}"))?;
        tx.commit()?;
        Ok(nueva)
    }

    pub fn bloquear_revision(&mut self, rev: i64, usuario: &str) -> Resultado<()> {
        let tx = self.con.transaction()?;
        tx.execute("UPDATE revisiones SET bloqueada = 1 WHERE id = ?1", [rev])?;
        auditar(&tx, rev, usuario, "bloquear_revision", "")?;
        tx.commit()?;
        Ok(())
    }

    /// Cambia el precio propio de un recurso básico en una revisión.
    pub fn actualizar_precio(&mut self, rev: i64, codigo: &str, precio: Decimal, usuario: &str) -> Resultado<()> {
        let tx = self.con.transaction()?;
        comprobar_editable(&tx, rev)?;
        let anterior: Option<String> = tx
            .query_row(
                "SELECT precio FROM conceptos WHERE revision_id = ?1 AND codigo = ?2",
                params![rev, codigo],
                |r| r.get(0),
            )
            .optional()?;
        let anterior = anterior.ok_or_else(|| ppto_core::ErrorMotor::ConceptoInexistente(codigo.into()))?;
        tx.execute(
            "UPDATE conceptos SET precio = ?3 WHERE revision_id = ?1 AND codigo = ?2",
            params![rev, codigo, precio.to_string()],
        )?;
        auditar(
            &tx,
            rev,
            usuario,
            "actualizar_precio",
            &format!("{codigo}: {anterior} → {precio}"),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn cargar_revision(&self, rev: i64) -> Resultado<Presupuesto> {
        let (nombre, raiz, decimales, ci) = self
            .con
            .query_row(
                "SELECT nombre, raiz, dec_dimensiones, dec_medicion, dec_rendimiento,
                        dec_importe_linea, dec_precio, dec_importe,
                        costes_indirectos, ci_redondear_coste_antes, ci_aplicar_sin_descomponer
                 FROM revisiones WHERE id = ?1",
                [rev],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        Decimales {
                            dimensiones: r.get(2)?,
                            medicion: r.get(3)?,
                            rendimiento: r.get(4)?,
                            importe_linea: r.get(5)?,
                            precio: r.get(6)?,
                            importe: r.get(7)?,
                        },
                        (r.get::<_, String>(8)?, r.get::<_, bool>(9)?, r.get::<_, bool>(10)?),
                    ))
                },
            )
            .optional()?
            .ok_or(ErrorDb::RevisionInexistente(rev))?;

        let mut conceptos = BTreeMap::new();
        let mut st = self.con.prepare(
            "SELECT codigo, unidad, resumen, naturaleza, precio, texto FROM conceptos WHERE revision_id = ?1",
        )?;
        let filas = st.query_map([rev], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        for f in filas {
            let (codigo, unidad, resumen, nat, precio, texto) = f?;
            conceptos.insert(
                codigo.clone(),
                Concepto {
                    codigo,
                    unidad,
                    resumen,
                    naturaleza: naturaleza_de(&nat)?,
                    precio: dec_de(&precio)?,
                    descomposicion: Vec::new(),
                    texto,
                },
            );
        }

        let mut st = self.con.prepare(
            "SELECT padre, hijo, factor, rendimiento FROM descomposicion
             WHERE revision_id = ?1 ORDER BY padre, orden",
        )?;
        let filas = st.query_map([rev], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for f in filas {
            let (padre, hijo, factor, rend) = f?;
            let c = conceptos
                .get_mut(&padre)
                .ok_or_else(|| ErrorDb::Corrupto(format!("padre {padre} sin concepto")))?;
            c.descomposicion.push(LineaDescomposicion {
                hijo,
                factor: dec_de(&factor)?,
                rendimiento: dec_de(&rend)?,
            });
        }

        let mut mediciones: BTreeMap<(String, String), Medicion> = BTreeMap::new();
        let mut st = self.con.prepare(
            "SELECT padre, hijo, tipo, comentario, unidades, longitud, anchura, altura, formula
             FROM medicion_lineas WHERE revision_id = ?1 ORDER BY padre, hijo, orden",
        )?;
        let filas = st.query_map([rev], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                [
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ],
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        for f in filas {
            let (padre, hijo, tipo, comentario, dims, formula) = f?;
            let [u, l, a, h] = dims.map(|v| v.map(|s| dec_de(&s)).transpose());
            mediciones.entry((padre, hijo)).or_default().lineas.push(LineaMedicion {
                tipo: tipo_de(&tipo)?,
                comentario,
                unidades: u?,
                longitud: l?,
                anchura: a?,
                altura: h?,
                formula,
            });
        }

        let (ci_txt, redondear_coste_antes, aplicar_a_sin_descomponer) = ci;
        Ok(Presupuesto {
            costes_indirectos: dec_de(&ci_txt)?,
            opciones_ci: OpcionesCi {
                redondear_coste_antes,
                aplicar_a_sin_descomponer,
            },
            nombre,
            decimales,
            raiz,
            conceptos,
            mediciones,
        })
    }

    /// Revisiones guardadas en el fichero, de la más antigua a la más reciente.
    pub fn listar_revisiones(&self) -> Resultado<Vec<InfoRevision>> {
        let mut st = self.con.prepare(
            "SELECT r.id, r.presupuesto_id, p.nombre, r.etiqueta, r.autor, r.creado_en, r.bloqueada
             FROM revisiones r JOIN presupuestos p ON p.id = r.presupuesto_id ORDER BY r.id",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(InfoRevision {
                    id: r.get(0)?,
                    presupuesto_id: r.get(1)?,
                    presupuesto: r.get(2)?,
                    etiqueta: r.get(3)?,
                    autor: r.get(4)?,
                    creado_en: r.get(5)?,
                    bloqueada: r.get::<_, i64>(6)? == 1,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Primera etiqueta «R<n>» libre para un presupuesto.
    pub fn siguiente_etiqueta(&self, presupuesto_id: i64) -> Resultado<String> {
        let mut n = 1;
        loop {
            let etiqueta = format!("R{n}");
            let existe: i64 = self.con.query_row(
                "SELECT COUNT(*) FROM revisiones WHERE presupuesto_id = ?1 AND etiqueta = ?2",
                params![presupuesto_id, etiqueta],
                |r| r.get(0),
            )?;
            if existe == 0 {
                return Ok(etiqueta);
            }
            n += 1;
        }
    }

    /// Operaciones registradas para una revisión: (usuario, operación, detalle).
    pub fn auditoria(&self, rev: i64) -> Resultado<Vec<(String, String, String)>> {
        let mut st = self
            .con
            .prepare("SELECT usuario, operacion, detalle FROM auditoria WHERE revision_id = ?1 ORDER BY id")?;
        let v = st
            .query_map([rev], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }
}

fn escribir_contenido(tx: &Transaction, rev: i64, p: &Presupuesto) -> Resultado<()> {
    {
        let mut st = tx.prepare(
            "INSERT INTO conceptos (revision_id, codigo, unidad, resumen, naturaleza, precio, texto)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for c in p.conceptos.values() {
            st.execute(params![
                rev,
                c.codigo,
                c.unidad,
                c.resumen,
                naturaleza_txt(c.naturaleza),
                c.precio.to_string(),
                c.texto
            ])?;
        }
    }
    {
        let mut st = tx.prepare(
            "INSERT INTO descomposicion (revision_id, padre, orden, hijo, factor, rendimiento)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for c in p.conceptos.values() {
            for (i, l) in c.descomposicion.iter().enumerate() {
                st.execute(params![
                    rev,
                    c.codigo,
                    i as i64,
                    l.hijo,
                    l.factor.to_string(),
                    l.rendimiento.to_string()
                ])?;
            }
        }
    }
    let mut st = tx.prepare(
        "INSERT INTO medicion_lineas (revision_id, padre, hijo, orden, tipo, comentario,
            unidades, longitud, anchura, altura, formula)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for ((padre, hijo), m) in &p.mediciones {
        for (i, l) in m.lineas.iter().enumerate() {
            let s = |v: Option<Decimal>| v.map(|x| x.to_string());
            st.execute(params![
                rev,
                padre,
                hijo,
                i as i64,
                tipo_txt(l.tipo),
                l.comentario,
                s(l.unidades),
                s(l.longitud),
                s(l.anchura),
                s(l.altura),
                l.formula
            ])?;
        }
    }
    Ok(())
}

fn comprobar_editable(tx: &Transaction, rev: i64) -> Resultado<()> {
    let b: Option<i64> = tx
        .query_row("SELECT bloqueada FROM revisiones WHERE id = ?1", [rev], |r| r.get(0))
        .optional()?;
    match b {
        None => Err(ErrorDb::RevisionInexistente(rev)),
        Some(1) => Err(ErrorDb::RevisionBloqueada(rev)),
        Some(_) => Ok(()),
    }
}

fn auditar(tx: &Transaction, rev: i64, usuario: &str, op: &str, detalle: &str) -> Resultado<()> {
    tx.execute(
        "INSERT INTO auditoria (revision_id, usuario, operacion, detalle) VALUES (?1, ?2, ?3, ?4)",
        params![rev, usuario, op, detalle],
    )?;
    Ok(())
}

fn columnas(tx: &Transaction, tabla: &str) -> Resultado<Vec<String>> {
    let mut st = tx.prepare(&format!("PRAGMA table_info({tabla})"))?;
    let v = st
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

fn dec_de(s: &str) -> Resultado<Decimal> {
    Decimal::from_str(s).map_err(|_| ErrorDb::Corrupto(format!("decimal no válido: {s}")))
}

const NATURALEZAS: [(Naturaleza, &str); 8] = [
    (Naturaleza::Capitulo, "capitulo"),
    (Naturaleza::Partida, "partida"),
    (Naturaleza::ManoObra, "mano_obra"),
    (Naturaleza::Maquinaria, "maquinaria"),
    (Naturaleza::Material, "material"),
    (Naturaleza::Subcontrata, "subcontrata"),
    (Naturaleza::Porcentaje, "porcentaje"),
    (Naturaleza::Otros, "otros"),
];

fn naturaleza_txt(n: Naturaleza) -> &'static str {
    NATURALEZAS
        .iter()
        .find(|(k, _)| *k == n)
        .map(|(_, v)| *v)
        .unwrap_or("otros")
}

fn naturaleza_de(s: &str) -> Resultado<Naturaleza> {
    NATURALEZAS
        .iter()
        .find(|(_, v)| *v == s)
        .map(|(k, _)| *k)
        .ok_or_else(|| ErrorDb::Corrupto(format!("naturaleza desconocida: {s}")))
}

fn tipo_txt(t: TipoLinea) -> &'static str {
    match t {
        TipoLinea::Normal => "normal",
        TipoLinea::SubtotalParcial => "subtotal_parcial",
        TipoLinea::SubtotalAcumulado => "subtotal_acumulado",
        TipoLinea::Formula => "formula",
    }
}

fn tipo_de(s: &str) -> Resultado<TipoLinea> {
    Ok(match s {
        "normal" => TipoLinea::Normal,
        "subtotal_parcial" => TipoLinea::SubtotalParcial,
        "subtotal_acumulado" => TipoLinea::SubtotalAcumulado,
        "formula" => TipoLinea::Formula,
        _ => return Err(ErrorDb::Corrupto(format!("tipo de línea desconocido: {s}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppto_core::ejemplos::suelo_radiante;
    use rust_decimal_macros::dec;

    #[test]
    fn migraciones_idempotentes() {
        let dir = std::env::temp_dir().join(format!("ppto-db-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        assert_eq!(
            Almacen::abrir(&dir).unwrap().version_esquema().unwrap(),
            MIGRACIONES.len()
        );
        assert_eq!(
            Almacen::abrir(&dir).unwrap().version_esquema().unwrap(),
            MIGRACIONES.len()
        );
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn ida_y_vuelta_sin_perdidas() {
        let mut a = Almacen::en_memoria().unwrap();
        let p = suelo_radiante();
        let id = a.crear_presupuesto("Vivienda tipo").unwrap();
        let r1 = a.guardar_revision(id, "R1", "juancar", &p).unwrap();
        let leido = a.cargar_revision(r1).unwrap();
        assert_eq!(leido, p);
        assert_eq!(leido.pem().unwrap(), dec!(7005.15));
    }

    #[test]
    fn modificar_r2_no_altera_r1() {
        let mut a = Almacen::en_memoria().unwrap();
        let id = a.crear_presupuesto("Vivienda tipo").unwrap();
        let r1 = a.guardar_revision(id, "R1", "juancar", &suelo_radiante()).unwrap();
        let r2 = a.derivar_revision(r1, "R2", "juancar").unwrap();
        a.actualizar_precio(r2, "MT.TUBO16", dec!(1.25), "juancar").unwrap();

        let p1 = a.cargar_revision(r1).unwrap();
        let p2 = a.cargar_revision(r2).unwrap();
        assert_eq!(p1, suelo_radiante());
        assert_eq!(p1.pem().unwrap(), dec!(7005.15));
        // Tubo 7 × 1,25 = 8,75; suma 30,35; MA 2 % = 0,607 → 0,61; precio 30,96 €/m²
        // 210 × 30,96 = 6.501,60 + colectores 728,25 = 7.229,85
        assert_eq!(p2.precio("SR.M2").unwrap(), dec!(30.96));
        assert_eq!(p2.pem().unwrap(), dec!(7229.85));

        let log = a.auditoria(r2).unwrap();
        assert_eq!(log[0].1, "derivar_revision");
        assert_eq!(
            log[1],
            (
                "juancar".into(),
                "actualizar_precio".into(),
                "MT.TUBO16: 1.10 → 1.25".into()
            )
        );
    }

    #[test]
    fn revision_bloqueada_no_admite_cambios() {
        let mut a = Almacen::en_memoria().unwrap();
        let id = a.crear_presupuesto("x").unwrap();
        let r1 = a.guardar_revision(id, "Oferta", "juancar", &suelo_radiante()).unwrap();
        a.bloquear_revision(r1, "juancar").unwrap();
        assert!(matches!(
            a.actualizar_precio(r1, "MT.TUBO16", dec!(9), "otro"),
            Err(ErrorDb::RevisionBloqueada(_))
        ));
        assert_eq!(a.cargar_revision(r1).unwrap(), suelo_radiante());
    }

    #[test]
    fn listado_y_siguiente_etiqueta() {
        let mut a = Almacen::en_memoria().unwrap();
        let id = a.crear_presupuesto("Vivienda").unwrap();
        assert_eq!(a.siguiente_etiqueta(id).unwrap(), "R1");
        let r1 = a.guardar_revision(id, "R1", "ana", &suelo_radiante()).unwrap();
        a.derivar_revision(r1, "R2", "luis").unwrap();
        a.bloquear_revision(r1, "ana").unwrap();
        assert_eq!(a.siguiente_etiqueta(id).unwrap(), "R3");
        let l = a.listar_revisiones().unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].etiqueta.as_str(), l[0].bloqueada), ("R1", true));
        assert_eq!((l[1].etiqueta.as_str(), l[1].autor.as_str()), ("R2", "luis"));
        assert_eq!(l[1].presupuesto, "Vivienda");
    }

    #[test]
    fn costes_indirectos_se_guardan_y_se_recuperan() {
        let mut a = Almacen::en_memoria().unwrap();
        let id = a.crear_presupuesto("x").unwrap();
        let mut p = suelo_radiante();
        p.costes_indirectos = dec!(45);
        p.opciones_ci.aplicar_a_sin_descomponer = false;
        let r1 = a.guardar_revision(id, "R1", "j", &p).unwrap();
        let r2 = a.derivar_revision(r1, "R2", "j").unwrap();
        for r in [r1, r2] {
            let leido = a.cargar_revision(r).unwrap();
            assert_eq!(leido, p);
            assert_eq!(leido.pem().unwrap(), dec!(10157.37));
        }
    }

    #[test]
    fn fichero_con_esquema_v1_se_actualiza_sin_perder_datos() {
        let ruta = std::env::temp_dir().join(format!("ppto-db-v1-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&ruta);
        {
            // Fichero creado por la versión anterior (solo migración 1)
            let con = Connection::open(&ruta).unwrap();
            con.execute_batch(MIGRACIONES[0]).unwrap();
            con.pragma_update(None, "user_version", 1).unwrap();
            con.execute_batch(
                "INSERT INTO presupuestos (id, nombre) VALUES (1, 'antiguo');
                 INSERT INTO revisiones (id, presupuesto_id, etiqueta, autor, nombre, raiz,
                    dec_dimensiones, dec_medicion, dec_rendimiento, dec_importe_linea, dec_precio, dec_importe)
                 VALUES (1, 1, 'R1', 'j', 'antiguo', 'OBRA', 2, 2, 3, 2, 2, 2);
                 INSERT INTO conceptos VALUES (1, 'OBRA', '', 'Obra', 'capitulo', '0', NULL);",
            )
            .unwrap();
        }
        let a = Almacen::abrir(&ruta).unwrap();
        assert_eq!(a.version_esquema().unwrap(), MIGRACIONES.len());
        let p = a.cargar_revision(1).unwrap();
        assert_eq!(p.costes_indirectos, Decimal::ZERO);
        assert_eq!(p.opciones_ci, OpcionesCi::default());
        drop(a);
        let _ = std::fs::remove_file(&ruta);
    }

    #[test]
    fn etiqueta_de_revision_unica() {
        let mut a = Almacen::en_memoria().unwrap();
        let id = a.crear_presupuesto("x").unwrap();
        a.guardar_revision(id, "R1", "j", &suelo_radiante()).unwrap();
        assert!(a.guardar_revision(id, "R1", "j", &suelo_radiante()).is_err());
    }
}
