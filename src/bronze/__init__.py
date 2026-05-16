"""
bronze/__init__.py
==================
v3.2 Bronze 層:純 raw 抓取,不做 pivot / pack。

模組結構(per blueprint v3.2 §三 + v3.5 R1 拆解):
  - phase_executor.py        Phase 1-6 排程(orchestration only)
  - segment_runner.py        單 segment fetch/transform/upsert 流程(v3.5 R1 C3 抽)
  - post_process_dividend.py 股利政策合併(v3.5 R1 C1 從 src/post_process.py 搬)
  - aggregators/             schema transform + business aggregation(v3.5 R1 C2 拆 package)
  - _common.py               filter_to_trading_days 共用 helper

PR #20 trigger 上線後 Bronze→Silver dirty 標記由 DB trigger
(`trg_mark_silver_dirty` / `trg_mark_fwd_silver_dirty`)接管;
PR #21 砍掉 Python 端的 deprecated dirty_marker shim。
"""
