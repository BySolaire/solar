import sys
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
f = TTFont(sys.argv[1])
s = instantiateVariableFont(f, {"wght": 500})
s.flavor = None
n = s["name"]
for rec in list(n.names):
    if rec.nameID in (16, 17, 21, 22, 25):
        n.removeNames(nameID=rec.nameID)
n.setName("Space Grotesk", 1, 3, 1, 0x409); n.setName("Regular", 2, 3, 1, 0x409)
n.setName("Space Grotesk Medium", 4, 3, 1, 0x409); n.setName("SpaceGrotesk-Medium", 6, 3, 1, 0x409)
n.setName("Space Grotesk", 16, 3, 1, 0x409); n.setName("Medium", 17, 3, 1, 0x409)
s["OS/2"].usWeightClass = 500
s.save(sys.argv[2])
t = TTFont(sys.argv[2]); print(t["name"].getDebugName(1), t["name"].getDebugName(17), t["OS/2"].usWeightClass, "fvar" in t, len(t.getBestCmap()))
