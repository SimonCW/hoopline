import { createInterface } from 'node:readline';

// Pipe chunks are not lines. Never accept a partial listener announcement.
export function onListening(stream, accept) {
  const lines = createInterface({ input: stream, crlfDelay: Infinity });
  lines.on('line', line => {
    const match = /^HOOPLINE_LISTENING=\S+:(\d+)$/.exec(line);
    if (match) accept(Number(match[1]));
  });
  return lines;
}
