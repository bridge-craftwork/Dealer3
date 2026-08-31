# Windows SSH Setup for Dealer.exe Testing

Setting up passwordless SSH from Mac to Windows 11 (Parallels) to run dealer.exe remotely.

## Current Status

✅ SSH key exists on Mac: `~/.ssh/id_ed25519.pub`
✅ SSH works with password
❌ SSH key authentication not working yet

## Your Public Key

Read it rather than copying it from here — this file is public, and a key
carries the username and hostname that generated it:

```bash
cat ~/.ssh/id_ed25519.pub
```

## Windows SSH Server Configuration

Windows OpenSSH has special rules for administrator accounts. Here's how to fix it:

### Step 1: Verify OpenSSH Server is Running

On your Windows machine, open PowerShell as Administrator:

```powershell
# Check if OpenSSH Server is installed
Get-WindowsCapability -Online | Where-Object Name -like 'OpenSSH.Server*'

# If not installed, install it:
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0

# Start the service
Start-Service sshd

# Set to start automatically
Set-Service -Name sshd -StartupType 'Automatic'
```

### Step 2: Configure Firewall

```powershell
# Verify firewall rule exists (should be automatic)
Get-NetFirewallRule -Name *ssh*

# If not, add it:
New-NetFirewallRule -Name sshd -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
```

### Step 3: Set Up authorized_keys

**IMPORTANT**: For administrator accounts, Windows uses a different location!

#### If your Windows user is an Administrator:

```powershell
# Location: C:\ProgramData\ssh\administrators_authorized_keys

# Create the file and add your public key
New-Item -Path "C:\ProgramData\ssh\administrators_authorized_keys" -ItemType File -Force

# Open in Notepad and paste your public key (the line from above)
notepad C:\ProgramData\ssh\administrators_authorized_keys

# Set correct permissions (CRITICAL!)
icacls "C:\ProgramData\ssh\administrators_authorized_keys" /inheritance:r
icacls "C:\ProgramData\ssh\administrators_authorized_keys" /grant "SYSTEM:(F)"
icacls "C:\ProgramData\ssh\administrators_authorized_keys" /grant "BUILTIN\Administrators:(F)"
```

#### If your Windows user is NOT an Administrator:

```powershell
# Location: %USERPROFILE%\.ssh\authorized_keys

# Create .ssh directory if it doesn't exist
New-Item -Path "$env:USERPROFILE\.ssh" -ItemType Directory -Force

# Create the file and add your public key
New-Item -Path "$env:USERPROFILE\.ssh\authorized_keys" -ItemType File -Force

# Open in Notepad and paste your public key
notepad $env:USERPROFILE\.ssh\authorized_keys

# Set correct permissions
icacls "$env:USERPROFILE\.ssh\authorized_keys" /inheritance:r
icacls "$env:USERPROFILE\.ssh\authorized_keys" /grant "$env:USERNAME:(F)"
```

### Step 4: Edit sshd_config (If Needed)

Open `C:\ProgramData\ssh\sshd_config` in Notepad as Administrator.

Make sure these lines are present and uncommented:

```
PubkeyAuthentication yes
PasswordAuthentication yes

# For administrator accounts, comment out these lines:
# Match Group administrators
#        AuthorizedKeysFile __PROGRAMDATA__/ssh/administrators_authorized_keys
```

**OR** if you want to use the administrators_authorized_keys approach, make sure those lines are **uncommented**.

### Step 5: Restart SSH Service

```powershell
Restart-Service sshd
```

### Step 6: Test from Mac

```bash
# Find your Windows IP address (from Windows: ipconfig)
ssh USERNAME@WINDOWS_IP

# Should connect without password!
```

## Troubleshooting

### Check SSH Logs on Windows

```powershell
# View recent SSH logs
Get-EventLog -LogName Application -Source OpenSSH -Newest 10
```

### Enable Verbose Logging (Windows)

Edit `C:\ProgramData\ssh\sshd_config`:

```
LogLevel DEBUG3
```

Then restart: `Restart-Service sshd`

Check logs at: `C:\ProgramData\ssh\logs\sshd.log`

### Test from Mac with Verbose Output

```bash
ssh -vvv USERNAME@WINDOWS_IP
```

Look for messages about:
- Key being offered
- Key being rejected
- Permission denied

## Common Issues

### Issue 1: Permissions Too Open

**Error**: "Permissions ... are too open"

**Fix**: See Step 3 - use icacls to set correct permissions

### Issue 2: Wrong authorized_keys Location

**Error**: Key works for non-admin but not admin (or vice versa)

**Fix**: Administrators use `C:\ProgramData\ssh\administrators_authorized_keys`

### Issue 3: Line Endings

**Error**: Key not recognized

**Fix**: Make sure authorized_keys file has Unix line endings (LF, not CRLF)
- Use Notepad++, not Windows Notepad
- Or use PowerShell to set the key properly

## Quick Setup Script for Windows

Save this as `setup-ssh-key.ps1` and run as Administrator:

```powershell
# Your public key (paste the actual key here)
# Paste the output of `cat ~/.ssh/id_ed25519.pub` on the Mac.
$publicKey = "ssh-ed25519 AAAA... user@host"

# For administrators
$authKeysFile = "C:\ProgramData\ssh\administrators_authorized_keys"

# Create file
New-Item -Path $authKeysFile -ItemType File -Force
Set-Content -Path $authKeysFile -Value $publicKey

# Set permissions
icacls $authKeysFile /inheritance:r
icacls $authKeysFile /grant "SYSTEM:(F)"
icacls $authKeysFile /grant "BUILTIN\Administrators:(F)"

# Restart SSH
Restart-Service sshd

Write-Host "SSH key setup complete!"
```

## Testing dealer.exe from Mac

Once SSH is working, you can run dealer.exe commands:

```bash
# Simple test
ssh USERNAME@WINDOWS_IP "C:\path\to\dealer.exe -V"

# Generate deals
echo "hcp(north) >= 15" | ssh USERNAME@WINDOWS_IP "C:\path\to\dealer.exe -p 10"

# Use input file
ssh USERNAME@WINDOWS_IP "C:\path\to\dealer.exe -p 100 < C:\path\to\test.dlr"
```

## Next Steps

Once SSH works:
1. Create wrapper script on Mac to run dealer.exe remotely
2. Generate test data for our Rust implementation
3. Compare outputs to verify compatibility
