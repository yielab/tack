---
title: El PDF se corta al imprimir facturas largas
type: bug
roles:
- Desarrollo
fields:
brief:
  definition_of_done: Una factura de 40 líneas se imprime completa
  risk: medium
  acceptance:
  - kind: command
    id: pdf-largo
    title: La prueba del PDF largo pasa
    run: cargo test pdf_largo
    expect_exit: 0
    cwd: null
---
