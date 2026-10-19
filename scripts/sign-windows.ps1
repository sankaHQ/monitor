# Tauri's Windows signCommand (written by setup-windows-signing.ps1): signs one file
# through Microsoft Artifact Signing and checks the result. Ported from Sanka Recorder.
param([Parameter(Mandatory)][string]$FilePath)

$ErrorActionPreference = 'Stop'
foreach ($name in @('MONITOR_SIGNTOOL_PATH', 'MONITOR_SIGNING_DLIB', 'MONITOR_SIGNING_METADATA')) {
  $value = [Environment]::GetEnvironmentVariable($name)
  if ([string]::IsNullOrWhiteSpace($value) -or -not (Test-Path -LiteralPath $value -PathType Leaf)) {
    throw "Missing signing tool or metadata: $name"
  }
}
$file = Get-Item -LiteralPath $FilePath
& $env:MONITOR_SIGNTOOL_PATH sign /v /fd SHA256 /tr 'http://timestamp.acs.microsoft.com' /td SHA256 /d 'Sanka Monitor' /dlib $env:MONITOR_SIGNING_DLIB /dmdf $env:MONITOR_SIGNING_METADATA $file.FullName
if ($LASTEXITCODE -ne 0) {
  throw "Microsoft SignTool failed for $($file.Name) with exit code $LASTEXITCODE."
}
$signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
if ($signature.Status -ne 'Valid') {
  throw "Invalid signature on $($file.Name): $($signature.StatusMessage)"
}
$publisher = $signature.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
if ($publisher -cne 'Sanka, Inc.' -or $null -eq $signature.TimeStamperCertificate) {
  throw "Expected a timestamped Sanka, Inc. signature on $($file.Name)."
}
Write-Host "Verified timestamped Sanka, Inc. signature: $($file.Name)"
