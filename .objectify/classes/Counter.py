from objectify import DoBase
from dataclasses import dataclass, field
from typing import List

@dataclass
class CounterState:
    value: int = 0
    history: List[int] = field(default_factory=list)

class Counter(DoBase[CounterState]):
    async def increment(self, by: int = 1) -> int:
        state = await self.get() or {}
        new_val = state.get("value", 0) + by
        hist = state.get("history", [])
        await self.set(CounterState(value=new_val, history=[*hist, new_val]))
        return new_val

    async def reset(self) -> None:
        await self.set(CounterState())
