#!/usr/bin/env python3
import re
import sys
import os
import subprocess

def get_pkg_name(prefix, version):
    """Maps tarball prefix to pkg-config name."""
    version = version.lstrip('v')
    
    mapping = {
        "wayland": "wayland-client",
        "wayland-protocols": "wayland-protocols",
        "libdrm": "libdrm",
        "libdrm-libdrm": "libdrm",
        "seatd": "libseat",
        "pixman": "pixman-1",
        "pixman-pixman": "pixman-1",
        "hwdata": "hwdata",
    }
    if prefix in mapping:
        return mapping[prefix]
    if prefix == "wlroots":
        v_parts = version.split('.')
        if len(v_parts) >= 2:
            return f"wlroots-{v_parts[0]}.{v_parts[1]}"
        return "wlroots"
    if prefix == "xserver-xwayland":
        return "xwayland"
    return prefix

def check_dependency(pkg, version):
    """Checks if a dependency is satisfied using pkg-config or binary check."""
    version = version.lstrip('v')
    install_dir = os.environ.get("INSTALL_DIR", "")
    
    if pkg == "xwayland":
        installed = get_installed_version("xwayland")
        if not installed:
            return False
        
        # Simple numeric version comparison (e.g. 22.1.9 >= 21.0.0)
        def ver_tuple(v):
            return tuple(map(int, (v.split('.') + [0, 0, 0])[:3]))
        
        try:
            return ver_tuple(installed) >= ver_tuple(version)
        except (ValueError, AttributeError):
            # Fallback to simple existence if version parsing fails
            return True

    env = os.environ.copy()
    if install_dir:
        pvc_pkg_config = os.path.join(install_dir, "lib/pkgconfig")
        pvc_share_pkg_config = os.path.join(install_dir, "share/pkgconfig")
        current_path = env.get("PKG_CONFIG_PATH", "")
        env["PKG_CONFIG_PATH"] = f"{pvc_pkg_config}:{pvc_share_pkg_config}:{current_path}"

    try:
        cmd = ["pkg-config", f"--atleast-version={version}", pkg]
        subprocess.check_call(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return True
    except (subprocess.CalledProcessError, FileNotFoundError):
        return False

def get_installed_version(pkg):
    """Gets the currently installed version of a package."""
    install_dir = os.environ.get("INSTALL_DIR", "")
    
    if pkg == "xwayland":
        binary = os.path.join(install_dir, "bin/Xwayland")
        if not os.path.exists(binary):
            binary = "/usr/bin/Xwayland"
        
        if os.path.exists(binary):
            try:
                # Xwayland -version prints to stderr and returns 0
                output = subprocess.check_output([binary, "-version"], stderr=subprocess.STDOUT).decode()
                # Match "Version X.Y.Z"
                m = re.search(r"Version\s+([0-9.]+)", output)
                if m:
                    return m.group(1)
                return "unknown-version"
            except (subprocess.CalledProcessError, FileNotFoundError):
                return "binary-found"
        return None

    env = os.environ.copy()
    if install_dir:
        pvc_pkg_config = os.path.join(install_dir, "lib/pkgconfig")
        pvc_share_pkg_config = os.path.join(install_dir, "share/pkgconfig")
        current_path = env.get("PKG_CONFIG_PATH", "")
        env["PKG_CONFIG_PATH"] = f"{pvc_pkg_config}:{pvc_share_pkg_config}:{current_path}"

    try:
        return subprocess.check_output(["pkg-config", "--modversion", pkg], env=env, stderr=subprocess.DEVNULL).decode().strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None

def parse_dependencies(script_content):
    """Extracts all (prefix, version) pairs from the script."""
    vars = {}
    for match in re.finditer(r'^([A-Z_]+)=([0-9.]+)', script_content, re.MULTILINE):
        vars[match.group(1)] = match.group(2)

    deps = []
    for match in re.finditer(r'tarball="([^"]+)\.tar\..*?"', script_content):
        full_name = match.group(1)
        
        # Resolve variables safely using regex boundaries ($VAR or ${VAR})
        def replacer(m):
            var_name = m.group(1) or m.group(2)
            return vars.get(var_name, m.group(0))
            
        full_name = re.sub(r'\$([A-Za-z_][A-Za-z0-9_]*)|\$\{([A-Za-z_][A-Za-z0-9_]*)\}', replacer, full_name)
        
        # Now full_name is expanded (e.g. wayland-1.24.0, libdrm-libdrm-2.4.122, 0.6.4, v0.364)
        
        # Some tarballs don't have a prefix (like seatd which is just $SEATD.tar.gz)
        # Or hwdata which is v$HWDATA.tar.gz
        if '-' not in full_name:
            # Try to guess prefix from the variable name used
            original_name = match.group(1)
            var_match = re.search(r'\$([A-Z_]+)', original_name)
            if var_match:
                prefix = var_match.group(1).lower()
                version = full_name
                deps.append((prefix, version))
            continue
            
        # Split on the first dash that is followed by a digit or 'v'
        m = re.search(r'^(.*?)-([v0-9].*)$', full_name)
        if not m:
            continue
            
        prefix = m.group(1)
        version = m.group(2)
        
        deps.append((prefix, version))
    return deps

def patch_script(input_path, output_path):
    with open(input_path, 'r') as f:
        content = f.read()
    
    lines = content.splitlines(keepends=True)

    # Build the shell-side check logic
    # We call back into this python script to get the mapping to avoid duplication
    this_script = os.path.realpath(__file__)
    check_logic = rf"""
should_build() {{
    tarball_expr=$1
    base=$(echo "$tarball_expr" | sed 's/\.tar\..*//')
    
    # Check if base has a dash
    if [[ "$base" == *-* ]]; then
        # prefix is everything before the first dash-digit or dash-v
        prefix=$(echo "$base" | sed -E 's/-[v0-9].*//')
        # version_str is everything from the first digit or v after a dash
        version_str=$(echo "$base" | sed -E 's/.*-([v0-9].*)/\1/')
    else
        # no prefix in filename (e.g. 0.6.4.tar.gz for seatd or v0.364.tar.gz for hwdata)
        version_str="$base"
        if [[ "$version_str" == v* ]]; then
            prefix="hwdata"
        else
            prefix="seatd"
        fi
    fi
    
    clean_v=${{version_str#v}}
    
    # Resolve package name using python helper
    pkg=$(python3 "{this_script}" dummy dummy --get-pkg "$prefix" "$clean_v")

    if [ "$pkg" = "xwayland" ]; then
        if python3 "{this_script}" dummy dummy --check-dep xwayland "$clean_v"; then
            echo "Xwayland $clean_v already satisfied, skipping build."
            return 1
        fi
    elif [ "$pkg" = "hwdata" ] || [[ "$version_str" == v* ]]; then
        if pkg-config --atleast-version="$clean_v" hwdata; then
            echo "hwdata $clean_v already satisfied, skipping build."
            return 1
        fi
    else
        if pkg-config --atleast-version="$clean_v" "$pkg"; then
            echo "$pkg $clean_v already satisfied, skipping build."
            return 1
        fi
    fi
    return 0
}}
"""

    new_lines = [check_logic]
    i = 0
    in_apt_group = False

    while i < len(lines):
        line = lines[i]
        
        if "apt update" in line and not in_apt_group:
            new_lines.append('echo "::group::🔄 Installing System Dependencies (apt)"\n')
            in_apt_group = True
            new_lines.append(line)
            i += 1
            continue

        tarball_match = re.search(r'tarball="([^"]+)"', line)
        if tarball_match:
            if in_apt_group:
                new_lines.append('echo "::endgroup::"\n')
                in_apt_group = False
                
            tarball_expr = tarball_match.group(1)
            
            # Extract a friendly name for the group header
            import re as _re
            base = _re.sub(r'\.tar\..*', '', tarball_expr)
            if '-' not in base:
                prefix = 'hwdata' if base.startswith('v') else 'seatd'
            else:
                prefix = _re.sub(r'-[v0-9\$].*', '', base)
            
            new_lines.append(line)
            new_lines.append(f'if should_build "{tarball_expr}"; then\n')
            new_lines.append(f'echo "::group::📦 Building {prefix} (from source)"\n')

            j = i + 1
            while j < len(lines):
                inner_line = lines[j]
                if "meson setup build" in inner_line:
                    inner_line = inner_line.replace("meson setup build", "meson setup build -Dwerror=false")
                if "./configure --prefix=/usr" in inner_line:
                    inner_line = inner_line.replace("--prefix=/usr", "--prefix=\"$INSTALL_DIR\"")
                    inner_line = inner_line.replace("--libdir=/lib", "--libdir=\"$INSTALL_DIR/lib\"")
                    inner_line = inner_line.replace("--datadir=/usr/share", "--datadir=\"$INSTALL_DIR/share\"")
                    inner_line = inner_line.replace("--pkgconfigdir=/usr/share/pkgconfig", "--pkgconfigdir=\"$INSTALL_DIR/share/pkgconfig\"")

                if re.search(r'^\s*cd \.\./?', inner_line) or re.search(r'tarball="([^"]+)"', inner_line):
                    if re.search(r'^\s*cd \.\./?', inner_line):
                        new_lines.append(inner_line)
                        new_lines.append("echo \"::endgroup::\"\n")
                        new_lines.append("fi\n")
                    else:
                        new_lines.append("echo \"::endgroup::\"\n")
                        new_lines.append("fi\n")
                        new_lines.append(inner_line)
                    i = j
                    break
                new_lines.append(inner_line)
                j += 1
            else:
                new_lines.append("echo \"::endgroup::\"\n")
                new_lines.append("fi\n")
                i = j - 1
            i += 1
            continue
        new_lines.append(line)
        i += 1

    with open(output_path, 'w') as f:
        f.writelines(new_lines)

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: patch_wayland_setup.py <input> <output> [--check-all] [--get-pkg <prefix> <version>] [--check-dep <pkg> <version>]")
        sys.exit(1)

    if "--get-pkg" in sys.argv:
        idx = sys.argv.index("--get-pkg")
        prefix = sys.argv[idx + 1]
        version = sys.argv[idx + 2]
        print(get_pkg_name(prefix, version))
        sys.exit(0)

    if "--check-dep" in sys.argv:
        idx = sys.argv.index("--check-dep")
        pkg = sys.argv[idx + 1]
        version = sys.argv[idx + 2]
        sys.exit(0 if check_dependency(pkg, version) else 1)

    input_file = sys.argv[1]
    output_file = sys.argv[2]
    
    if "--check-all" in sys.argv:
        with open(input_file, 'r') as f:
            content = f.read()
        dependencies = parse_dependencies(content)
        all_ok = True
        
        # We know these are standard, anything else might be new
        known_prefixes = {"wayland", "wayland-protocols", "libdrm", "seatd", "pixman", "hwdata", "wlroots", "xserver-xwayland", "libdrm-libdrm", "pixman-pixman"}
        
        print("::group::Dependency Verification (--check-all)")
        for prefix, version in dependencies:
            pkg = get_pkg_name(prefix, version)
            clean_v = version.lstrip('v')
            installed_ver = get_installed_version(pkg)
            
            if prefix not in known_prefixes:
                print(f"::warning::⚠️ NEW DEPENDENCY DETECTED in upstream script: {prefix} ({clean_v}) -> mapped to {pkg}")
            
            if not check_dependency(pkg, clean_v):
                if installed_ver:
                    print(f"::warning::🔄 VERSION BUMP DETECTED for {pkg}: installed {installed_ver}, needs >= {clean_v}")
                print(f"❌ Dependency NOT satisfied: {pkg} (needs >= {clean_v})")
                all_ok = False
            else:
                print(f"✅ Dependency satisfied: {pkg} (installed {installed_ver})")
        print("::endgroup::")
        
        sys.exit(0 if all_ok else 1)

    patch_script(input_file, output_file)
