from websockets.client import connect
import asyncio
import signal
import json
import numpy as np
import sys
import math

TURN_RATE = 10
SCAN_RANGE = 3

async def play(address: str):
    async with connect(address) as websocket:
                # Close the connection when receiving SIGTERM.
        loop = asyncio.get_running_loop()
        loop.add_signal_handler(
            signal.SIGTERM, loop.create_task, websocket.close())
        try:
            frame = 0
            async for data in websocket:
                frame += 1
                self_id, ships, bullets, map, killfeed = json.loads(data)
                self_id = str(self_id)
                self_ship = ships.get(self_id, None)
                if self_ship is None: # dead or not in game
                    await websocket.send(json.dumps({"MoveShip": {"angle": 0.0}}))
                    continue
                if frame % TURN_RATE == 0:
                    observation = np.array(map)
                    x, y = round(self_ship["x"]) % observation.shape[1], round(self_ship["y"]) % observation.shape[0]
                    ids = observation[np.ix_(np.arange(y - SCAN_RANGE, y + SCAN_RANGE + 1, 1) % observation.shape[0], np.arange(x - SCAN_RANGE, x + SCAN_RANGE + 1, 1) % observation.shape[1])]
                    bullets_y, bullets_x = np.where(ids < 0)
                    if bullets_x.size > 0:
                        dx = -bullets_x + SCAN_RANGE
                        dy = -bullets_y + SCAN_RANGE
                        angle = np.arctan2(dy, dx)
                        await websocket.send(json.dumps({"MoveShip": {"angle": angle.mean()}}))
        finally:
            await websocket.close()

async def multiple_play(count: int):
    await asyncio.gather(*(play("ws://localhost:48666") for _ in range(count)))

if __name__ == "__main__":
    count = 1
    if len(sys.argv) > 1:
        count = int(sys.argv[1])
    asyncio.run(multiple_play(count))
    