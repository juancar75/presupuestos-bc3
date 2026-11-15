-- SPDX-License-Identifier: GPL-3.0-or-later
-- Partida incluida en otra (importe cero, sale como «INCLUIDO»).
ALTER TABLE conceptos ADD COLUMN incluida INTEGER NOT NULL DEFAULT 0;
