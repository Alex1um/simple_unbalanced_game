from websockets.client import connect
import asyncio
import signal
import json
import math
import random
import sys

TURN_RATE = 60
ATTACK_RATE = 10

def choose_target_and_rel_pos(self_id, ships: dict[str, dict]):
    if len(ships) > 1:
        ids = random.sample(tuple(ships.keys()), 2)
        target_id = ids[0] if ids[0] != self_id else ids[1]
        relative_x = random.randint(-3, 3)
        relative_y = random.randint(-3, 3)
        return target_id, relative_x, relative_y
    return None, 0, 0


async def play(address: str):
    async with connect(address) as websocket:
                # Close the connection when receiving SIGTERM.
        loop = asyncio.get_running_loop()
        loop.add_signal_handler(
            signal.SIGTERM, loop.create_task, websocket.close())
        try:
            frame = 0
            relative_x = 0
            relative_y = 0
            target_id = None
            async for data in websocket:
                frame += 1
                id, ships, bullets, map, killfeed = json.loads(data)
                id = str(id)
                self_ship = ships.get(id, None)
                if self_ship is None: # dead or not in game
                    target_id, relative_x, relative_y = choose_target_and_rel_pos(id, ships)
                    await websocket.send(json.dumps({"MoveShip": {"angle": 0.0}}))
                    continue
                if target_id in ships:
                    target_ship = ships[target_id]
                    target_x, target_y = target_ship["x"], target_ship["y"]
                    if frame % TURN_RATE == 0:
                        angle = math.atan2(target_y + relative_y - self_ship["y"], target_x + relative_x - self_ship["x"])
                        await websocket.send(json.dumps({"MoveShip": {"angle": angle}}))
                    if frame % ATTACK_RATE == 0 and math.dist((self_ship["x"], self_ship["y"]), (target_x, target_y)) < 7:
                        target_dirrection = target_ship["angle"]
                        target_x += math.cos(target_dirrection) * 2
                        target_y += math.sin(target_dirrection) * 2
                        angle = math.atan2(target_y - self_ship["y"], target_x - self_ship["x"])    
                        await websocket.send(json.dumps({"AddBullet": {"angle": angle}}))
                else:
                    target_id, relative_x, relative_y = choose_target_and_rel_pos(id, ships)
        finally:
            await websocket.close()

async def multiple_play(count: int):
    await asyncio.gather(*(play("ws://localhost:48666") for _ in range(count)))

if __name__ == "__main__":
    count = 1
    if len(sys.argv) > 1:
        count = int(sys.argv[1])
    asyncio.run(multiple_play(count))
    