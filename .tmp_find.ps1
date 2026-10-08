$p = Get-CimInstance Win32_Process -Filter "Name='smartdns.exe'" -ErrorAction SilentlyContinue
if ($p) {
    $p | ForEach-Object { Write-Host "PID=$($_.ProcessId)"; Write-Host "PATH=$($_.ExecutablePath)"; Write-Host "CMD=$($_.CommandLine)" }
} else {
    Write-Host 'no smartdns process'
}
Write-Host '== services =='
Get-CimInstance Win32_Service -ErrorAction SilentlyContinue | Where-Object { $_.Name -match 'smartdns' -or $_.PathName -match 'smartdns' } | ForEach-Object {
    Write-Host "service=$($_.Name) state=$($_.State) startmode=$($_.StartMode)"
    Write-Host "path=$($_.PathName)"
}
