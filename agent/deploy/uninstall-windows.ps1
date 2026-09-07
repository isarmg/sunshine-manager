#Requires -RunAsAdministrator
$ErrorActionPreference = 'Stop'
$binDir = Join-Path $env:ProgramFiles 'SunshineAgent'
$target = Join-Path $binDir 'sunshine-agent.exe'
if ((Get-Item -LiteralPath $binDir).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unexpected installation link' }
Stop-Service SunshineAgent
(Get-Service SunshineAgent).WaitForStatus('Stopped',[TimeSpan]::FromSeconds(30))
& sc.exe delete SunshineAgent
if ($LASTEXITCODE -ne 0) { throw 'Service deletion failed' }
Remove-Item -LiteralPath $target
# Nonrecursive: refuse to delete unrelated files.
[IO.Directory]::Delete($binDir,$false)
Write-Host 'Agent executable/service removed. ProgramData\SunshineAgent retained for recovery and deduplication. Revoke the device in Manager.'
