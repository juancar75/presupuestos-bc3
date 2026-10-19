-- SPDX-License-Identifier: GPL-3.0-or-later
-- Precio de venta bloqueado de la partida; NULL = calculado con el margen.
ALTER TABLE conceptos ADD COLUMN venta_fija TEXT;
