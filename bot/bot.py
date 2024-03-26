from websockets.client import connect
import asyncio
import signal
import json
import math

TURN_RATE = 60
ATTACK_RATE = 10

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
                id, ships, bullets, map, killfeed = json.loads(data)
                id = str(id)
                self_ship = ships.get(id, None)
                if self_ship is None:
                    await websocket.send(json.dumps({"MoveShip": {"angle": 0.0}}))
                    continue
                for ship_id, ship in ships.items():
                    if ship_id != id:
                        dest_x, dest_y = ship["x"], ship["y"]
                        if frame % TURN_RATE == 0:
                            angle = math.atan2(dest_y + 2 - self_ship["y"], dest_x - self_ship["x"])
                            await websocket.send(json.dumps({"MoveShip": {"angle": angle}}))
                        if frame % ATTACK_RATE == 0 and math.dist((self_ship["x"], self_ship["y"]), (dest_x, dest_y)) < 7:
                            target_dirrection = ship["angle"]
                            target_x, target_y = ship["x"], ship["y"]
                            target_x += math.cos(target_dirrection) * 3
                            target_y += math.sin(target_dirrection) * 3
                            angle = math.atan2(target_y - self_ship["y"], target_x - self_ship["x"])    
                            await websocket.send(json.dumps({"AddBullet": {"angle": angle}}))
                        break
        finally:
            await websocket.close()

if __name__ == "__main__":
    asyncio.run(play("ws://localhost:48666"))
    