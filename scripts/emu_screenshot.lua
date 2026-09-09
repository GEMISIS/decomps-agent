-- FCEUX Lua: run N frames, optionally press Start/A, save a screenshot, exit.
-- Env: NESSHOT_OUT (png path), NESSHOT_FRAMES (default 240), NESSHOT_PRESS (button name pressed on frame 120..130)
local out    = os.getenv("NESSHOT_OUT") or "screenshot.png"
local frames = tonumber(os.getenv("NESSHOT_FRAMES") or "240")
local press  = os.getenv("NESSHOT_PRESS")
local press_at = tonumber(os.getenv("NESSHOT_PRESS_AT") or "120")
for i = 1, frames do
  if press and ((i >= press_at and i <= press_at + 2) or (i >= press_at + 40 and i <= press_at + 42)) then
    local j = {A=false,B=false,select=false,start=false,up=false,down=false,left=false,right=false}; j[press] = true; joypad.set(1, j)
  end
  emu.frameadvance()
end
gui.savescreenshotas(out)
emu.frameadvance()
print("saved " .. out)
-- no emu.exit(): it segfaults the Qt build (crash report per run); the shell wrapper stops the emulator with SIGTERM once the output file exists

