#!/usr/bin/env python3
import re
import sys

def patch_script(input_path, output_path):
    with open(input_path, 'r') as f:
        lines = f.readlines()

    new_lines = []
    
    # Inject check logic at the top of the file
    check_logic = """
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
            v="$(echo "$tarball_var" | grep -oP '\\d+\\.\\d+')"
            pkg="wlroots-$v" ;;
        xserver-xwayland-*) pkg="xwayland" ; v="$XWAYLAND" ;;
        *) pkg="" ; v="" ;;
    esac

    if [ -n "$pkg" ]; then
        # For wlroots, check both wlroots-X.Y and generic wlroots
        if [[ "$pkg" == wlroots-* ]]; then
            if pkg-config --atleast-version="$v" "$pkg" && pkg-config --atleast-version="$v" wlroots; then
                echo "$pkg already installed in PVC, skipping build."
                return 1
            fi
        # For xwayland, check the binary directly
        elif [[ "$pkg" == xwayland ]]; then
            if command -v Xwayland &>/dev/null; then
                 echo "Xwayland already installed in PVC, skipping build."
                 return 1
            fi
        # For others, just pkg-config
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
        
        # Look for tarball assignments to identify the start of a dependency block
        tarball_match = re.search(r'tarball="([^"]+)"', line)
        if tarball_match:
            tarball_expr = tarball_match.group(1)
            new_lines.append(line)
            new_lines.append(f'if should_build "{tarball_expr}"; then\n')
            
            # Consume lines until we find the 'cd ../' or the next 'tarball=' or EOF
            j = i + 1
            while j < len(lines):
                inner_line = lines[j]
                if re.search(r'^\s*cd \.\.', inner_line) or re.search(r'tarball="([^"]+)"', inner_line):
                    new_lines.append(inner_line)
                    new_lines.append("fi\n")
                    i = j
                    break
                new_lines.append(inner_line)
                j += 1
            else:
                # Reached end of file, close the last block
                new_lines.append("fi\n")
                i = j - 1 # Ensure i is at EOF

            i += 1 # Move past the tarball line we just processed
            continue
        
        new_lines.append(line)
        i += 1

    with open(output_path, 'w') as f:
        f.writelines(new_lines)

if __name__ == "__main__":
    patch_script(sys.argv[1], sys.argv[2])
