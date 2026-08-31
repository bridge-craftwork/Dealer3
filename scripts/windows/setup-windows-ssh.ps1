# PowerShell script to set up SSH key authentication on Windows
# Run this as Administrator on your Windows 11 Parallels VM

# Your Mac's public SSH key. Not committed: a public key carries the username and
# hostname that generated it, and this repository is public.
# Set MAC_PUBLIC_KEY, or paste the output of `cat ~/.ssh/id_ed25519.pub`.
$publicKey = $env:MAC_PUBLIC_KEY
if (-not $publicKey) {
    Write-Host "ERROR: set MAC_PUBLIC_KEY to your Mac's public key first." -ForegroundColor Red
    Write-Host "  On the Mac:  cat ~/.ssh/id_ed25519.pub" -ForegroundColor Yellow
    Write-Host '  On Windows:  $env:MAC_PUBLIC_KEY = "ssh-ed25519 AAAA... user@host"' -ForegroundColor Yellow
    exit 1
}

Write-Host "Setting up SSH key authentication on Windows..." -ForegroundColor Cyan
Write-Host ""

# Check if running as Administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "ERROR: This script must be run as Administrator!" -ForegroundColor Red
    Write-Host "Right-click PowerShell and select 'Run as Administrator'" -ForegroundColor Yellow
    exit 1
}

# Step 1: Check if OpenSSH Server is installed
Write-Host "Step 1: Checking OpenSSH Server..." -ForegroundColor Yellow
$sshServer = Get-WindowsCapability -Online | Where-Object Name -like 'OpenSSH.Server*'

if ($sshServer.State -ne "Installed") {
    Write-Host "Installing OpenSSH Server..." -ForegroundColor Yellow
    Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
} else {
    Write-Host "[OK] OpenSSH Server is installed" -ForegroundColor Green
}

# Step 2: Start and configure SSH service
Write-Host ""
Write-Host "Step 2: Configuring SSH service..." -ForegroundColor Yellow
Start-Service sshd
Set-Service -Name sshd -StartupType 'Automatic'
Write-Host "[OK] SSH service is running and set to start automatically" -ForegroundColor Green

# Step 3: Configure firewall
Write-Host ""
Write-Host "Step 3: Checking firewall..." -ForegroundColor Yellow
$firewallRule = Get-NetFirewallRule -Name *ssh* -ErrorAction SilentlyContinue
if (-not $firewallRule) {
    New-NetFirewallRule -Name sshd -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
    Write-Host "[OK] Firewall rule created" -ForegroundColor Green
} else {
    Write-Host "[OK] Firewall rule already exists" -ForegroundColor Green
}

# Step 4: Determine if current user is Administrator
Write-Host ""
Write-Host "Step 4: Setting up authorized_keys..." -ForegroundColor Yellow

$currentUser = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object System.Security.Principal.WindowsPrincipal($currentUser)
$isUserAdmin = $principal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)

if ($isUserAdmin) {
    Write-Host "Detected: Administrator account" -ForegroundColor Yellow

    # For administrators: use C:\ProgramData\ssh\administrators_authorized_keys
    $authKeysFile = "C:\ProgramData\ssh\administrators_authorized_keys"

    # Create directory if needed
    $sshDir = Split-Path $authKeysFile
    if (-not (Test-Path $sshDir)) {
        New-Item -Path $sshDir -ItemType Directory -Force | Out-Null
    }

    # Create/update authorized_keys file
    Set-Content -Path $authKeysFile -Value $publicKey -Encoding ASCII
    Write-Host "[OK] Created $authKeysFile" -ForegroundColor Green

    # Set permissions (CRITICAL!)
    icacls $authKeysFile /inheritance:r | Out-Null
    icacls $authKeysFile /grant "SYSTEM:(F)" | Out-Null
    icacls $authKeysFile /grant "BUILTIN\Administrators:(F)" | Out-Null
    Write-Host "[OK] Set permissions on authorized_keys" -ForegroundColor Green

} else {
    Write-Host "Detected: Regular user account" -ForegroundColor Yellow

    # For regular users: use %USERPROFILE%\.ssh\authorized_keys
    $authKeysFile = "$env:USERPROFILE\.ssh\authorized_keys"

    # Create .ssh directory
    $sshDir = "$env:USERPROFILE\.ssh"
    if (-not (Test-Path $sshDir)) {
        New-Item -Path $sshDir -ItemType Directory -Force | Out-Null
    }

    # Create/update authorized_keys file
    Set-Content -Path $authKeysFile -Value $publicKey -Encoding ASCII
    Write-Host "[OK] Created $authKeysFile" -ForegroundColor Green

    # Set permissions
    icacls $authKeysFile /inheritance:r | Out-Null
    icacls $authKeysFile /grant "$env:USERNAME:(F)" | Out-Null
    Write-Host "[OK] Set permissions on authorized_keys" -ForegroundColor Green
}

# Step 5: Restart SSH service
Write-Host ""
Write-Host "Step 5: Restarting SSH service..." -ForegroundColor Yellow
Restart-Service sshd
Write-Host "[OK] SSH service restarted" -ForegroundColor Green

# Step 6: Display connection info
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "SSH Setup Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Your Windows IP addresses:" -ForegroundColor Yellow
Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -notlike "127.*" -and $_.IPAddress -notlike "169.*"} | ForEach-Object {
    Write-Host "  $($_.IPAddress)" -ForegroundColor White
}
Write-Host ""
Write-Host "Current user: $env:USERNAME" -ForegroundColor Yellow
Write-Host ""
Write-Host "Test from your Mac with:" -ForegroundColor Yellow
Write-Host "  ssh $env:USERNAME@<IP_ADDRESS>" -ForegroundColor White
Write-Host ""
Write-Host "If it asks for a password, check WINDOWS_SSH_SETUP.md for troubleshooting" -ForegroundColor Yellow
