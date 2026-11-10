# Called by Tauri for each Windows binary before it is packed: the app, the
# engine sidecar, NSIS plugins, the uninstaller and the final setup .exe.
# Copied from Support-Phone's scripts/sign-windows.ps1. Azure Trusted Signing,
# account Titanium, profile titanium-public, through the Azure CLI session that
# azure/login (GitHub OIDC) opened before `tauri build`.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string] $Path
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$file = (Get-Item -LiteralPath $Path).FullName

Import-Module TrustedSigning -RequiredVersion 0.5.8
$signing = @{
    Endpoint = 'https://eus.codesigning.azure.net/'
    CodeSigningAccountName = 'Titanium'
    CertificateProfileName = 'titanium-public'
    Files = $file
    FileDigest = 'SHA256'
    TimestampRfc3161 = 'http://timestamp.acs.microsoft.com'
    TimestampDigest = 'SHA256'
    ExcludeEnvironmentCredential = $true
    ExcludeWorkloadIdentityCredential = $true
    ExcludeManagedIdentityCredential = $true
    ExcludeSharedTokenCacheCredential = $true
    ExcludeVisualStudioCredential = $true
    ExcludeVisualStudioCodeCredential = $true
    ExcludeAzureCliCredential = $false
    ExcludeAzurePowerShellCredential = $true
    ExcludeAzureDeveloperCliCredential = $true
    ExcludeInteractiveBrowserCredential = $true
}
Invoke-TrustedSigning @signing

# A successful service call is not enough. Fail the build if Windows does not
# trust the signature, or the publisher or timestamp is missing.
$signature = Get-AuthenticodeSignature -LiteralPath $file
if ($signature.Status -ne 'Valid' -or
    $null -eq $signature.SignerCertificate -or
    $signature.SignerCertificate.Subject -notmatch 'Titanium Computing' -or
    $null -eq $signature.TimeStamperCertificate) {
    throw "Invalid Titanium Computing Authenticode signature or timestamp: $file"
}
Write-Host "Verified Titanium Computing signature: $file"
