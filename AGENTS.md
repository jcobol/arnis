# Agent Notes: Rail Placement Pitfalls

This project relies on `WorldEditor` helper methods to place blocks into the
Minecraft world. The editor can silently refuse to overwrite an existing block,
so keep these rules in mind when adding automated content:

1. `WorldEditor::set_block` and `WorldEditor::set_block_with_properties`
   compute the absolute Y position, then check if a block is already present.
   If something is there and no override whitelist/blacklist is supplied, the
   placement is skipped. Gravel foundations placed earlier in the same loop can
   therefore prevent later calls (such as the redstone power source) from
   taking effect.
2. When you need to replace an existing block intentionally, pass an override
   whitelist that includes the block you expect to replace, or use the
   `_absolute` variants if you already have the world height figured out.
3. Powered rails do not need a redstone block below them when the NBT
   properties set `powered=true`. In our generator we explicitly set that flag
   when we place the rail, so the rail still works even if the redstone block
   was skipped.
4. When debugging missing blocks, search for earlier `set_block` calls along
   the same coordinates and check whether override parameters are provided.
   The order of operations inside a loop often matters more than it first
   appears.

Following these guidelines should help avoid silent block placement failures in
future automation work.

# Code Comments

For complex code or calculations, adding code comments can help for future reference.

