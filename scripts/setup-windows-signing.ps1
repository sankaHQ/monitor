# Prepares Authenticode signing for the Windows release job (docs/windows.md#release):
# Microsoft's pinned Artifact Signing client, Windows SDK SignTool, the signing metadata
# and the Tauri signCommand override. Ported from Sanka Recorder's release workflow.
$ErrorActionPreference = 'Stop'

foreach ($name in @('AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET', 'AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_ACCOUNT', 'AZURE_SIGNING_PROFILE')) {
  if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
    throw "Missing signing setting: $name"
  }
}

# Pin the Microsoft client package; never execute an unchecked download with signing credentials.
$version = '1.0.146'
$expectedHash = '7761e5d8f31e54b764da21d33cd8b0a2940e1b05526c7dfcb5e94b0431426f70'
$directory = Join-Path $env:RUNNER_TEMP 'monitor-signing'
New-Item -ItemType Directory -Force -Path $directory | Out-Null
$archive = Join-Path $directory 'client.zip'
Invoke-WebRequest -Uri "https://api.nuget.org/v3-flatcontainer/microsoft.artifactsigning.client/$version/microsoft.artifactsigning.client.$version.nupkg" -OutFile $archive
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $expectedHash) {
  throw 'Microsoft Artifact Signing client checksum mismatch.'
}
$client = Join-Path $directory 'client'
Expand-Archive -LiteralPath $archive -DestinationPath $client -Force
$dlib = Join-Path $client 'bin/x64/Azure.CodeSigning.Dlib.dll'
if (-not (Test-Path -LiteralPath $dlib -PathType Leaf)) {
  throw 'The Microsoft package did not contain the x64 signing library.'
}

$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$signTool = Get-ChildItem -Path "$sdk/*/x64/signtool.exe" -File |
  Where-Object { [version]$_.Directory.Parent.Name -ge [version]'10.0.22621.0' } |
  Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
  Select-Object -First 1
if ($null -eq $signTool) {
  throw 'A supported x64 Windows SDK SignTool was not found.'
}

# Use only the approved CI credential. No interactive or Azure CLI subscription login.
$metadata = Join-Path $directory 'metadata.json'
@{
  Endpoint = $env:AZURE_SIGNING_ENDPOINT
  CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
  CertificateProfileName = $env:AZURE_SIGNING_PROFILE
  CorrelationId = $env:GITHUB_RUN_ID
  ExcludeCredentials = @(
    'ManagedIdentityCredential', 'WorkloadIdentityCredential', 'SharedTokenCacheCredential',
    'VisualStudioCredential', 'VisualStudioCodeCredential', 'AzureCliCredential',
    'AzurePowerShellCredential', 'AzureDeveloperCliCredential', 'InteractiveBrowserCredential'
  )
} | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $metadata -Encoding utf8
@(
  "MONITOR_SIGNTOOL_PATH=$($signTool.FullName)"
  "MONITOR_SIGNING_DLIB=$dlib"
  "MONITOR_SIGNING_METADATA=$metadata"
) | Add-Content -LiteralPath $env:GITHUB_ENV -Encoding utf8

# Tauri's string notation splits on spaces, including quoted descriptions. Use argv.
$configuration = @{
  bundle = @{
    windows = @{
      signCommand = @{
        cmd = 'pwsh'
        args = @('-NoProfile', '-NonInteractive', '-File', (Join-Path $PSScriptRoot 'sign-windows.ps1'), '-FilePath', '%1')
      }
    }
  }
}
$configuration | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $PSScriptRoot '../apps/agent/src-tauri/tauri.signing.conf.json') -Encoding utf8
Write-Host "Microsoft Artifact Signing client $version and SDK $($signTool.Directory.Parent.Name) configured."
