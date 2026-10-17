# Packs the built Windows app into an MSIX for the Microsoft Store. The package is not signed:
# the Store signs it when it is uploaded to Partner Center.
# The identity values come from Partner Center > the app > Product identity.
#   scripts/msix.ps1 -Version 0.7.0 -IdentityName <Package/Identity/Name> `
#     -Publisher <Package/Identity/Publisher> -PublisherName <Package/Properties/PublisherDisplayName>
param(
  [Parameter(Mandatory)] [string] $Version,
  [Parameter(Mandatory)] [string] $IdentityName,
  [Parameter(Mandatory)] [string] $Publisher,
  [Parameter(Mandatory)] [string] $PublisherName,
  [string] $Exe = "$PSScriptRoot\..\target\release\tenun-app.exe",
  [string] $Out = "$PSScriptRoot\..\target\release\bundle\msix"
)
$ErrorActionPreference = 'Stop'

# The Store wants four numbers, and the last one must be 0.
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must look like 1.2.3, not $Version" }
$v4 = "$Version.0"

$kits = 'C:\Program Files (x86)\Windows Kits\10\bin'
$makeappx = Get-ChildItem $kits -Recurse -Filter makeappx.exe |
  Where-Object { $_.Directory.Name -eq 'x64' } | Sort-Object FullName | Select-Object -Last 1
if (-not $makeappx) { throw 'makeappx.exe not found. Install the Windows SDK.' }

$layout = Join-Path $Out 'layout'
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory "$layout\Assets" | Out-Null
Copy-Item $Exe "$layout\tenun-app.exe"
$icons = "$PSScriptRoot\..\app\src-tauri\icons"
foreach ($i in 'StoreLogo', 'Square44x44Logo', 'Square71x71Logo', 'Square150x150Logo') {
  Copy-Item "$icons\$i.png" "$layout\Assets\$i.png"
}

$esc = [System.Security.SecurityElement]
@"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">
  <Identity Name="$($esc::Escape($IdentityName))" Publisher="$($esc::Escape($Publisher))" Version="$v4" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Tenun</DisplayName>
    <PublisherDisplayName>$($esc::Escape($PublisherName))</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="en-us" />
    <Resource Language="id-id" />
  </Resources>
  <Applications>
    <Application Id="Tenun" Executable="tenun-app.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Tenun" Description="Weave your spreadsheets and databases into one dashboard."
        BackgroundColor="transparent" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png">
        <uap:DefaultTile Square71x71Logo="Assets\Square71x71Logo.png" />
      </uap:VisualElements>
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
"@ | Set-Content -Encoding utf8 "$layout\AppxManifest.xml"

$msix = Join-Path $Out "Tenun_${Version}_x64.msix"
& $makeappx.FullName pack /d $layout /p $msix /o
if ($LASTEXITCODE -ne 0) { throw 'makeappx failed' }
Remove-Item -Recurse -Force $layout
$msix
