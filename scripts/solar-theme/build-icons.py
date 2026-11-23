"""Option C icons from Identité's SVGs. args: <option-c dir> <repo> <iconset out>
From 64 px up: the option C tile. At 16 and 32 px: the compact tile from
<option-c dir>/petites-tailles (thicker, enlarged S)."""
import sys, io, re, cairosvg
from PIL import Image
D, R, ISET = sys.argv[1:4]
tile_svg = open(D + '/solar-icone.svg').read()
small_svg = open(D + '/petites-tailles/solar-icone-petite.svg').read()
tile_for = lambda size: small_svg if size <= 32 else tile_svg
# Identité's 16 and 32 px exports are drawn on the pixel grid: use them as is.
SMALL = {px: Image.open(f'{D}/petites-tailles/solar-icone-petite-{px}.png').convert('RGBA') for px in (16, 32)}
def small_or_render(svg, px):
    return SMALL[px].copy() if px in SMALL and svg is small_svg else render(svg, px)
def render(svg, size):
    return Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg.encode(), output_width=size, output_height=size))).convert('RGBA')
# 1. macOS 26+ layer: the white S on transparent, the system adds the fill and mask.
render(open(D + '/solar-icone-calque.svg').read(), 1024).save(R + '/src-tauri/macos/AppIcon.icon/Assets/icon.png', optimize=True)
# 2. Flat tiles (Windows, Linux, Tauri sizes): Identité's tile as drawn.
for name, size in [('32x32', 32), ('128x128', 128), ('128x128@2x', 256), ('icon', 256), ('Square30x30Logo', 30), ('Square44x44Logo', 44),
                   ('Square71x71Logo', 71), ('Square89x89Logo', 89), ('Square107x107Logo', 107), ('Square142x142Logo', 142),
                   ('Square150x150Logo', 150), ('Square284x284Logo', 284), ('Square310x310Logo', 310), ('StoreLogo', 50)]:
    small_or_render(tile_for(size) if name == '32x32' else tile_svg, size).save(f'{R}/src-tauri/icons/{name}.png', optimize=True)
render(tile_svg, 1024).save(R + '/public/solar.png', optimize=True)
ico = [small_or_render(tile_for(px), px) for px in (16, 24, 32, 48, 64, 128, 256)]
ico[-1].save(R + '/src-tauri/icons/icon.ico', sizes=[im.size for im in ico], append_images=ico[:-1])
# 3. Classic macOS .icns (before macOS 26): Apple's grid, the tile at 824 of 1024.
def grid(svg, side):
    return re.sub(r'<svg[^>]*>', f'<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><g transform="translate(100 100) scale({824 / side})">', svg, count=1).replace('</svg>', '</g></svg>')
for pt in [16, 32, 128, 256, 512]:
    for px, suffix in ((pt, ''), (pt * 2, '@2x')):
        # 16 and 32 px: the pixel-aligned compact exports, full size (Apple's
        # grid would scale them off the pixel grid); from 64 px, the C tile.
        (SMALL[px].copy() if px <= 32 else render(grid(tile_svg, 1024), px)).save(f'{ISET}/icon_{pt}x{pt}{suffix}.png')
print('ok')
