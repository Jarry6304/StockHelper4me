"""
bronze/__init__.py
==================
v3.2 Bronze 層:純 raw 抓取,不做 pivot / pack。

模組結構(per blueprint v3.2 §三):
  - dirty_marker.py    Bronze 寫入後在 Silver dirty queue 標記(短期路徑;
                       PR #20 trigger 上線後 deprecate)
  - phase_executor.py  從 src/phase_executor.py 拆出(PR #19c 動工)

PR #19a 落地 dirty_marker stub;phase_executor 拆段留 PR #19c。
"""
