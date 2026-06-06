declare module 'plotly.js-dist-min' {
  // 第三方無官方 dist-min @types;runtime API 與 plotly.js 對齊。
  // 原型階段用寬鬆 any default export;production 階段可改成 import('plotly.js') 並
  // tree-shake 個別模組。
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Plotly: any;
  export default Plotly;
}
