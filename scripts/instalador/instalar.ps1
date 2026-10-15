# Instala presupuestos-bc3 para el usuario actual (sin permisos de administrador).
# - Copia los programas a %LOCALAPPDATA%\Programs\presupuestos-bc3
# - Crea accesos directos en el escritorio y en el menú Inicio
# - Si está Claude Desktop, añade el servidor MCP «presupuestos» (con copia de seguridad)
$ErrorActionPreference = 'Stop'
$origen  = Split-Path -Parent $MyInvocation.MyCommand.Path
$destino = Join-Path $env:LOCALAPPDATA 'Programs\presupuestos-bc3'

Write-Host "Instalando en $destino ..."
Get-Process ppto-gui, ppto-mcp -ErrorAction SilentlyContinue | Stop-Process -Force
New-Item -ItemType Directory -Force -Path $destino | Out-Null
foreach ($f in 'ppto-gui.exe', 'ppto-mcp.exe', 'ppto.exe', 'LEEME.txt') {
    $p = Join-Path $origen $f
    if (Test-Path $p) {
        Copy-Item $p $destino -Force
        Unblock-File (Join-Path $destino $f) -ErrorAction SilentlyContinue
    }
}

# Accesos directos
$gui = Join-Path $destino 'ppto-gui.exe'
$shell = New-Object -ComObject WScript.Shell
$sitios = @(
    [Environment]::GetFolderPath('Desktop'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) '')
)
foreach ($d in $sitios) {
    $lnk = $shell.CreateShortcut((Join-Path $d 'Presupuestos BC3.lnk'))
    $lnk.TargetPath = $gui
    $lnk.WorkingDirectory = $destino
    $lnk.Save()
}
Write-Host "Accesos directos creados: escritorio y menú Inicio («Presupuestos BC3»)."

# Claude Desktop (instalación normal o de Microsoft Store)
$mcp = Join-Path $destino 'ppto-mcp.exe'
$carpetas = @(Join-Path $env:APPDATA 'Claude')
$carpetas += Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Packages') -Directory -Filter 'Claude_*' -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName 'LocalCache\Roaming\Claude' }
$hecho = $false
foreach ($c in $carpetas) {
    if (-not (Test-Path $c)) { continue }
    $cfg = Join-Path $c 'claude_desktop_config.json'
    if (Test-Path $cfg) {
        $texto = Get-Content $cfg -Raw -Encoding UTF8
        if ([string]::IsNullOrWhiteSpace($texto)) { $texto = '{}' }
        try { $json = $texto | ConvertFrom-Json }
        catch {
            Write-Warning "No se pudo leer $cfg (JSON no válido). No se toca; añada el servidor a mano (ver LEEME.txt)."
            continue
        }
        Copy-Item $cfg "$cfg.antes-de-presupuestos" -Force
    } else {
        $json = [pscustomobject]@{}
    }
    if (-not $json.PSObject.Properties['mcpServers']) {
        $json | Add-Member -NotePropertyName mcpServers -NotePropertyValue ([pscustomobject]@{})
    }
    $srv = [pscustomobject]@{ command = $mcp }
    if ($json.mcpServers.PSObject.Properties['presupuestos']) {
        $json.mcpServers.presupuestos = $srv
    } else {
        $json.mcpServers | Add-Member -NotePropertyName presupuestos -NotePropertyValue $srv
    }
    $salida = $json | ConvertTo-Json -Depth 32
    [IO.File]::WriteAllText($cfg, $salida, (New-Object Text.UTF8Encoding $false))
    Write-Host "Claude Desktop configurado: $cfg"
    $hecho = $true
}
if (-not $hecho) {
    Write-Host "No se ha encontrado Claude Desktop. El programa funciona igual; si lo instala después, vuelva a ejecutar instalar.bat."
} else {
    Write-Host "Cierre Claude Desktop del todo (icono junto al reloj > Salir) y ábralo de nuevo para que cargue «presupuestos»."
}
Write-Host ""
Write-Host "Listo. Abra «Presupuestos BC3» desde el escritorio."
