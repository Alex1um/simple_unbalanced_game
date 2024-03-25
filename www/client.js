const ws = new WebSocket(`ws://25.20.226.178:48666`); // Replace with your server address

const canvas = document.getElementById("gameCanvas");
const ctx = canvas.getContext("2d");
const cellSize = Math.min(canvas.width / 20, canvas.height / 20);
var ship_x = 0;
var ship_y = 0;
var dest_x = 0;
var dest_y = 0;

ws.onopen = () => {
  console.log("Connected to server!");
  // Handle user input (example)
  document.addEventListener("keydown", (event) => {
    if (event.key === "ArrowUp") {
      sendAction({ "MoveShip":  {angle: 0.1} }); // Replace values as needed
    } else if (event.key === " ") {
      sendAction({ "AddBullet": {angle: Math.PI / 2} }); // Replace values as needed
    } else if (event.key === "Escape") {
      ws.close();
    }
  });
  canvas.oncontextmenu = (event) => {
    event.preventDefault();
    const rect = canvas.getBoundingClientRect()
    let click_x = event.clientX - rect.left;
    let click_y = event.clientY - rect.top;
    let x = click_x - ship_x * cellSize - cellSize / 2;
    let y = click_y - ship_y * cellSize - cellSize / 2;
    let angle = -Math.atan2(x, y) + Math.PI / 2;
    dest_x = (click_x - cellSize / 2) / cellSize;
    dest_y = (click_y - cellSize / 2) / cellSize;
    sendAction({ "MoveShip":  {angle: angle} });
  }
  canvas.addEventListener("click", (event) => {
    console.log(event);
    const rect = canvas.getBoundingClientRect()
    let click_x = event.clientX - rect.left;
    let click_y = event.clientY - rect.top;
    let x = click_x - ship_x * cellSize - cellSize / 2;
    let y = click_y - ship_y * cellSize - cellSize / 2;
    let angle = -Math.atan2(x, y) + Math.PI / 2;
    if (event.button === 0) {
      sendAction({ "AddBullet": {angle: angle} }); // Replace values as needed
    }
  })
};


function renderState(id, ships, bullets, map) {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
  
  
    // Render ships and bullets
    ships.forEach((value, key, map) => {
      console.log(key, id)
      const centerX = value.x * cellSize + cellSize / 2;
      const centerY = value.y * cellSize + cellSize / 2;

      if (key == id) {
        ctx.fillStyle = "green";
      } else {
        ctx.fillStyle = "blue";
      }

      ctx.fillRect(centerX - cellSize / 4, centerY - cellSize / 4, cellSize / 2, cellSize / 2);
    });
    bullets.forEach((element, key, map) => {
      const centerX = element["x"] * cellSize + cellSize / 2;
      const centerY = element["y"] * cellSize + cellSize / 2;
      ctx.fillStyle = "red";
      ctx.beginPath();
      ctx.ellipse(centerX, centerY, cellSize / 5, cellSize / 8, element["angle"], 0, Math.PI * 2);
      ctx.closePath();
      ctx.fill();
    });
    let current_ship = ships.get(id.toString());
    if (current_ship == null) {
      return;
    }
    ctx.textAlign = "end";
    // ctx.lineWidth = 2;
    ctx.fillStyle = "white";
    const font_size = 20;
    ctx.font = `${font_size}px`;
    Object.entries(current_ship).forEach((value, key, data) => { 
      ctx.fillText(`${value}`, canvas.width, font_size + key * font_size);
    });
    ship_x = current_ship["x"];
    ship_y = current_ship["y"];
    // if (Math.abs(ship_x - dest_x) < 0.1 && Math.abs(ship_y - dest_y) < 0.1) {
    //   sendAction({ "MoveShip": {id: 0, angle: 0} });
    // }
  }
ws.onmessage = (message) => {
  const state = JSON.parse(message.data);
  console.log(state)
  // Update your game state based on the received State object (ships, bullets, map)
  var [id, ships, bullets, map, dmgfeed] = state;
  ships = new Map(Object.entries(ships));
  bullets = new Map(Object.entries(bullets));

  renderState(id, ships, bullets, map);

};

function sendAction(action) {
  const jsonAction = JSON.stringify(action);
  ws.send(jsonAction);
}
