// Reads the Comix request keys out of the live site. Run it in a page of comix.to (after the page loaded) and it
// resolves to the JSON that `Keys::from_json` reads: {"build","prefix","inner","outer","table"}.
// How it works: the site's `secure-*.js` module signs API requests with HMAC-SHA256 whose inner and outer states it
// builds with `Uint32Array.from` (captured here), and decrypts `{"e":...}` responses by XOR with a 1024-byte table
// (read here by handing the site's own decoder a forged response of zeros).
(async () => {
  const main = document.querySelector('script[type=module][src*="/dist/main-"]').src;
  const mainJs = await (await fetch(main)).text();
  const files = Array.from(mainJs.matchAll(/from\s*["']\.\/([^"']+\.js)["']/g), (m) => m[1]);
  const secureFile = files.find((f) => f.startsWith('secure-'));
  const secure = await import(new URL(secureFile, main).href);
  const hooks = [];
  secure.r({ interceptors: { request: { use: (f) => hooks.push(f) }, response: { use: () => {} } } });

  const states = [];
  const from = Uint32Array.from;
  Uint32Array.from = function (...args) {
    const out = from.apply(this, args);
    if (out.length === 8) states.push(Array.from(out));
    return out;
  };
  let token;
  try {
    token = (await hooks[0]({ url: '/manga', baseURL: '/api/v1', method: 'get', params: { page: 1 }, headers: {} })).params._;
  } finally {
    Uint32Array.from = from;
  }

  let api = null;
  for (const f of files) {
    const mod = await import(new URL(f, main).href);
    api = Object.values(mod).find((v) => v && typeof v === 'object' && typeof v.chapters === 'function');
    if (api) break;
  }
  const zeros = btoa(String.fromCharCode(...new Uint8Array(2 + 1024)));
  const forged = JSON.stringify({ e: zeros });
  const open = XMLHttpRequest.prototype.open;
  const decode = TextDecoder.prototype.decode;
  let table = null;
  XMLHttpRequest.prototype.open = function () {
    Object.defineProperty(this, 'responseText', { get: () => forged, configurable: true });
    Object.defineProperty(this, 'response', { get: () => forged, configurable: true });
    Object.defineProperty(this, 'status', { get: () => 200, configurable: true });
    // The site decrypts a body only when the response carries `x-enc: 2`.
    this.getAllResponseHeaders = () => 'content-type: application/json\r\nx-enc: 2\r\n';
    this.getResponseHeader = (name) => ({ 'content-type': 'application/json', 'x-enc': '2' })[String(name).toLowerCase()] ?? null;
    return open.apply(this, arguments);
  };
  TextDecoder.prototype.decode = function (bytes) {
    if (bytes && bytes.length === 1024) table = Array.from(bytes);
    return decode.call(this, bytes);
  };
  try {
    await api.chapters('x', { page: 1 }).catch(() => {});
  } finally {
    XMLHttpRequest.prototype.open = open;
    TextDecoder.prototype.decode = decode;
  }
  const hex = (a) => a.map((x) => x.toString(16).padStart(2, '0')).join('');
  return JSON.stringify({
    build: (secureFile.match(/secure-([^-]+)-/) || [])[1] || '',
    prefix: token.split('.')[0],
    inner: states[0],
    outer: states[1],
    table: table && hex(table),
  });
})()
