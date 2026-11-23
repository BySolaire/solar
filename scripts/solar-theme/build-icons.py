"""Option C icons from Identité's SVGs. args: <option-c dir> <repo> <iconset out>"""
import sys, io, re, cairosvg
from PIL import Image
D, R, ISET = sys.argv[1:4]
tile_svg = open(D + '/solar-icone.svg').read()
def render(svg, size):
    return Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg.encode(), output_width=size, output_height=size))).convert('RGBA')
# 1. macOS 26+ layer: the white S on transparent, the system adds the fill and mask.
render(open(D + '/solar-icone-calque.svg').read(), 1024).save(R + '/src-tauri/macos/AppIcon.icon/Assets/icon.png', optimize=True)
# 2. Flat tiles (Windows, Linux, Tauri sizes): Identité's tile as drawn.
for name, size in [('32x32', 32), ('128x128', 128), ('128x128@2x', 256), ('icon', 256), ('Square30x30Logo', 30), ('Square44x44Logo', 44),
                   ('Square71x71Logo', 71), ('Square89x89Logo', 89), ('Square107x107Logo', 107), ('Square142x142Logo', 142),
                   ('Square150x150Logo', 150), ('Square284x284Logo', 284), ('Square310x310Logo', 310), ('StoreLogo', 50)]:
    render(tile_svg, size).save(f'{R}/src-tauri/icons/{name}.png', optimize=True)
render(tile_svg, 1024).save(R + '/public/solar.png', optimize=True)
big = render(tile_svg, 256)
big.save(R + '/src-tauri/icons/icon.ico', sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
# 3. Classic macOS .icns (before macOS 26): Apple's grid, the tile at 824 of 1024.
inner = re.sub(r'<svg[^>]*>', '<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><g transform="translate(100 100) scale(0.8046875)">', tile_svg, count=1).replace('</svg>', '</g></svg>')
for pt in [16, 32, 128, 256, 512]:
    render(inner, pt).save(f'{ISET}/icon_{pt}x{pt}.png'); render(inner, pt * 2).save(f'{ISET}/icon_{pt}x{pt}@2x.png')
print('ok')
