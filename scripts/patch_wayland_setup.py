#!/usr/bin/env python3
import re
import sys

def patch_script(input_path, output_path):
    with open(input_path, 'r') as f:
        lines = f.readlines()

    new_lines = []
    
    # Inject check logic at the top of the file
    check_logic = r"""
should_build() {
    tarball_var=$1
    case "$tarball_var" in
        wayland-*) pkg="wayland-client" ; v="$WAYLAND" ;;
        wayland-protocols-*) pkg="wayland-protocols" ; v="$WAYLAND_PROTOCOLS" ;;
        libdrm-*) pkg="libdrm" ; v="$LIBDRM" ;;
        seatd-*) pkg="seatd" ; v="$SEATD" ;;
        pixman-*) pkg="pixman-1" ; v="$PIXMAN" ;;
        hwdata-*) pkg="hwdata" ; v="$HWDATA" ;;
        wlroots-*) 
            v="$(echo "$tarball_var" | grep -oP '\d+\.\d+')"
            pkg="wlroots-$v" ;;
        xserver-xwayland-*) pkg="xwayland" ; v="$XWAYLAND" ;;
        *) pkg="" ; v="" ;;
    esac

    if [ -n "$pkg" ]; then
        if [[ "$pkg" == wlroots-* ]]; then
            if pkg-config --atleast-version="$v" "$pkg" && pkg-config --atleast-version="$v" wlroots; then
                echo "$pkg already installed in PVC, skipping build."
                return 1
            fi
        elif [[ "$pkg" == xwayland ]]; then
            if [ -f "$INSTALL_DIR/bin/Xwayland" ]; then
                 echo "Xwayland already installed in PVC, skipping build."
                 return 1
            fi
        else
            if pkg-config --atleast-version="$v" "$pkg"; then
                echo "$pkg already installed in PVC, skipping build."
                return 1
            fi
        fi
    fi
    return 0
}
"""
    new_lines.append(check_logic)

    i = 0
    while i < len(lines):
        line = lines[i]
        
        tarball_match = re.search(r'tarball="([^"]+)"', line)
        if tarball_match:
            tarball_expr = tarball_match.group(1)
            new_lines.append(line)
            new_lines.append('if should_build "')
            new_lines.append(tarball_expr)
            new_lines.append('"; then\n')
            
            j = i + 1
            while j < len(lines):
                inner_line = lines[j]
                
                if "meson setup build" in inner_line:
                    inner_line = inner_line.replace("meson setup build", "meson setup build -Dwerror=false")
                
                if re.search(r'^\s*cd \.\.', inner_line) or re.search(r'tarball="([^"]+)"', inner_line):
                    if "cd .." in inner_line:
                        new_lines.append(inner_line)
                        new_lines.append("fi\n")
                    else:
                        new_lines.append("fi\n")
                        new_lines.append(inner_line)
                    i = j
                    break
                
                new_lines.append(inner_line)
                j += 1
            else:
                new_lines.append("fi\n")
                i = j - 1

            i += 1 
            continue
        
        new_lines.append(line)
        i += 1

    with open(output_path, 'w') as f:
        f.writelines(new_lines)

if __name__ == "__main__":
    patch_script(sys.argv[1], sys.argv[2])
