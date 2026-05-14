"""Plotly Figure → PNG bytes via kaleido。

kaleido 內含 chromium binary(首次用 plotly_get_chrome 安裝 ~80MB),
後續 render ~300-800ms per figure。

對齊 plan Phase D §figure_to_png helper。
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import plotly.graph_objects as go


DEFAULT_WIDTH = 1280
DEFAULT_HEIGHT = 800


def figure_to_png(
    fig: "go.Figure",
    *,
    width: int = DEFAULT_WIDTH,
    height: int = DEFAULT_HEIGHT,
) -> bytes:
    """Plotly Figure → PNG bytes。

    Args:
        fig: plotly graph_objects.Figure。
        width / height: 輸出尺寸(像素)。預設 1280×800 對齊 Desktop chat 顯示。

    Returns:
        PNG bytes。MCP Image content 可直接用。

    Raises:
        RuntimeError: kaleido 未安裝或 chromium binary 沒裝(訊息提示
            `plotly_get_chrome` 安裝指令)。
    """
    try:
        return fig.to_image(
            format="png",
            width=width,
            height=height,
            engine="kaleido",
        )
    except Exception as e:  # plotly 對缺 chromium 拋 RuntimeError;對缺 kaleido 拋 ValueError
        msg = str(e)
        if "Chrome" in msg or "chromium" in msg.lower():
            raise RuntimeError(
                "Kaleido 缺 chromium binary。請執行:\n"
                "    plotly_get_chrome -y\n"
                "(首次安裝約 ~80MB,後續本機快取)\n\n"
                f"原始 error: {e}"
            ) from e
        if "kaleido" in msg.lower():
            raise RuntimeError(
                "kaleido 未安裝。請執行:\n"
                "    pip install -e \".[mcp]\"\n"
                "或:\n"
                "    pip install kaleido>=0.2\n\n"
                f"原始 error: {e}"
            ) from e
        raise
