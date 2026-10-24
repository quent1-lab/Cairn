// Sonde bout à bout : se connecte, lit Hello, s'abonne à une fenêtre, compte les images.
const url = process.argv[2] ?? 'ws://127.0.0.1:8090/ws';
const half = BigInt(process.argv[3] ?? '2000');
const secs = Number(process.argv[4] ?? '10');
const zz = v => (v << 1n) ^ (v >> 63n);
function varint(v, out) { v = BigInt.asUintN(64, v); while (v >= 0x80n) { out.push(Number(v & 0x7fn) | 0x80); v >>= 7n; } out.push(Number(v)); }
function readVarint(b, i) { let v = 0n, s = 0n; for (;;) { const x = b[i++]; v |= BigInt(x & 0x7f) << s; s += 7n; if (!(x & 0x80)) return [v, i]; } }
const ws = new WebSocket(url); ws.binaryType = 'arraybuffer';
let frames = 0, bytes = 0, t0 = 0, sizes = [];
ws.onmessage = e => {
  const b = new Uint8Array(e.data);
  if (b[0] === 0) { // Hello
    let [proto, i] = readVarint(b, 1); let seed; [seed, i] = readVarint(b, i); let tick; [tick, i] = readVarint(b, i);
    console.log(`Hello protocole ${proto} seed ${seed} tick ${tick}`);
    // Subscribe tout le monde d'abord (comme le client)
    const out = [0]; for (const v of [-(2n**63n), -(2n**63n), 2n**63n-1n, 2n**63n-1n]) varint(zz(v), out);
    ws.send(new Uint8Array(out)); t0 = Date.now();
  } else if (b[0] === 1) { frames++; bytes += b.length; sizes.push(b.length);
    if (frames === 2) { // puis une fenêtre : centre inconnu ici, on garde « tout » pour la mesure
    }
  }
};
setTimeout(() => { const dt = (Date.now() - t0) / 1000;
  console.log(`${frames} images en ${dt.toFixed(1)} s (${(frames/dt).toFixed(1)} Hz), ${(bytes/Math.max(frames,1)).toFixed(0)} o/image, min ${Math.min(...sizes)} max ${Math.max(...sizes)}`);
  process.exit(0); }, secs * 1000);
ws.onerror = e => { console.log('erreur', e.message); process.exit(1); };
