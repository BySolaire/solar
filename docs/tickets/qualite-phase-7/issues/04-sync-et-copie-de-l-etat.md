Status: needs-triage

# 04 : `Shell::sync` copie tout l'état à chaque notification

`sync` clone l'état entier pour savoir s'il a changé, et tourne à chaque notification d'un onglet, donc pendant tout le streaming. Avec quelques dizaines de conversations c'est négligeable ; avec des centaines et de longs plans, ce sont des copies de plusieurs centaines de Ko plusieurs fois par seconde. À mesurer avant de changer (un compteur ou une trace sur un état gonflé) ; si c'est réel, marquer l'état modifié au lieu de comparer.
