# Device management and the Sanka Monitor app

The Sanka Monitor app reads a computer's security settings and software. It never manages
the computer: it can't install profiles, change settings, lock or erase anything, and it
has no root helper or background service that could. That doesn't change.

Your company may also manage its Macs from monitor.sanka.com with Sanka Monitor device
management. That is macOS's own device management (MDM), and it is separate from the app:

- **It starts only with your approval.** IT sends you an enroll link. The page it opens says
  what IT can and can't do, then gives you an enrollment profile. Nothing happens until you
  install that profile and approve it in System Settings. The app never installs it for you.
- **You can see it at any time.** On macOS 15 or later: System Settings → General → Device
  Management. On macOS 13 or 14: System Settings → Privacy & Security → Profiles.
  `profiles status -type enrollment` in Terminal shows whether the Mac is enrolled.
- **You can leave.** An administrator of the Mac can remove the profile there. macOS then
  removes every profile it installed, and your IT team sees that management was removed.

## What device management allows

The enrollment profile grants only these rights:

| IT can | IT can't |
| --- | --- |
| Require FileVault, the firewall, a screen lock, Gatekeeper and automatic updates | See your files, documents, messages, browsing, screen or location |
| Read the Mac's name, model, serial number, macOS version, security settings and installed profiles | See which apps you use or have installed |
| Keep the Mac's FileVault recovery key and bootstrap token, encrypted | Read network details or change other settings |
| Lock or erase the Mac | Install apps |

A feature that needs more rights would ask you to install a new profile.

The FileVault recovery key is kept as macOS encrypts it. Only your company's IT admins can
reveal it, each time with a reason that is logged and emailed to every admin. macOS also gives
its MDM a bootstrap token. It is kept encrypted and returned only when that Mac asks for it;
no one can view it.

## How the app reports it

The app's **Device management** check (`device_management`) passes when the Mac is enrolled in
any MDM, Sanka Monitor's or another one. It reads `profiles status -type enrollment` with your
own rights and changes nothing.

To deploy the app itself through Jamf Pro, Kandji or Intune, see
[mdm-deployment.md](mdm-deployment.md).
