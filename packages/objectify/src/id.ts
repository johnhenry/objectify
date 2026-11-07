import { randomBytes } from 'node:crypto';

export function newId(): string {
  return randomBytes(16).toString('hex');
}
