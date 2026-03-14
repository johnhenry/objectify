import { execFileSync } from 'node:child_process';
import { writeFileSync, unlinkSync, readFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { randomBytes } from 'node:crypto';
import type { ClassResult } from './types.js';

type ClassLang = 'ts' | 'py';

interface ClassFile {
  path: string;
  lang: ClassLang;
}

export function findClassFile(
  classesDir: string,
  className: string,
): ClassFile {
  const ts = join(classesDir, `${className}.ts`);
  if (existsSync(ts)) return { path: ts, lang: 'ts' };

  const py = join(classesDir, `${className}.py`);
  if (existsSync(py)) return { path: py, lang: 'py' };

  throw new Error(
    `class '${className}' not found. Expected ${className}.ts or ${className}.py in ${classesDir}`,
  );
}

function tempFile(prefix: string, suffix: string): string {
  const name = `${prefix}${randomBytes(8).toString('hex')}${suffix}`;
  return join(tmpdir(), name);
}

function generateTsRunner(classPath: string): string {
  return `import UserClass from '${classPath}';

const input = JSON.parse(Deno.env.get('OBJECTIFY_INPUT') || 'null');
const currentState = JSON.parse(Deno.env.get('OBJECTIFY_STATE') || '{}');

let pendingState = currentState;
let stateChanged = false;

const instance = new UserClass();

(instance as any).get = async () => structuredClone(currentState);
(instance as any).set = async (next: unknown) => {
  pendingState = next;
  stateChanged = true;
};

const method = Deno.env.get('OBJECTIFY_METHOD')!;
const result = await (instance as any)[method](input);

const output = {
  result: result ?? null,
  stateChanged,
  state: stateChanged ? pendingState : null,
};

console.log(JSON.stringify(output));
`;
}

function generatePyRunner(classPath: string, className: string): string {
  return `import sys, asyncio, json, os, importlib.util, inspect, types, copy

try:
    import dataclasses as _dc
    _has_dc = True
except ImportError:
    _has_dc = False

# Inject fake 'objectify' module
class _DoBase:
    def __init__(self):
        self._get_fn = None
        self._set_fn = None

    async def get(self):
        return await self._get_fn()

    async def set(self, state):
        return await self._set_fn(state)

    def __class_getitem__(cls, _item):
        return cls

_objectify_mod = types.ModuleType('objectify')
_objectify_mod.DoBase = _DoBase
sys.modules['objectify'] = _objectify_mod

# Load user class
_spec = importlib.util.spec_from_file_location("_uc", "${classPath}")
_module = importlib.util.module_from_spec(_spec)
_module.__package__ = ""
_spec.loader.exec_module(_module)
UserClass = getattr(_module, "${className}", None)
if UserClass is None:
    for _name, _obj in inspect.getmembers(_module, inspect.isclass):
        if _name != '_DoBase' and issubclass(_obj, _DoBase) and _obj is not _DoBase:
            UserClass = _obj
            break
if UserClass is None:
    print(json.dumps({"error": "class '${className}' not found in file"}), file=sys.stderr)
    sys.exit(1)

_current_state = json.loads(os.environ.get('OBJECTIFY_STATE', '{}'))
_method_name = os.environ.get('OBJECTIFY_METHOD', '')
_input = json.loads(os.environ.get('OBJECTIFY_INPUT', 'null'))

_pending_state = copy.deepcopy(_current_state)
_state_changed = False

_instance = UserClass()

async def _get_fn():
    return copy.deepcopy(_current_state)

async def _set_fn(state):
    global _pending_state, _state_changed
    if hasattr(state, 'model_dump'):
        _pending_state = state.model_dump()
    elif _has_dc and _dc.is_dataclass(state) and not isinstance(state, type):
        _pending_state = _dc.asdict(state)
    elif isinstance(state, dict):
        _pending_state = state
    else:
        _pending_state = state
    _state_changed = True

_instance._get_fn = _get_fn
_instance._set_fn = _set_fn

_method = getattr(_instance, _method_name, None)
if _method is None:
    print(json.dumps({"error": f"method '{_method_name}' not found on {UserClass.__name__}"}), file=sys.stderr)
    sys.exit(1)

def _serialize_result(val):
    if val is None:
        return None
    if hasattr(val, 'model_dump'):
        return val.model_dump()
    if _has_dc and _dc.is_dataclass(val) and not isinstance(val, type):
        return _dc.asdict(val)
    if isinstance(val, (list, tuple)):
        return [_serialize_result(v) for v in val]
    return val

if isinstance(_input, dict):
    if inspect.iscoroutinefunction(_method):
        _result = asyncio.run(_method(**_input))
    else:
        _result = _method(**_input)
elif _input is not None:
    if inspect.iscoroutinefunction(_method):
        _result = asyncio.run(_method(_input))
    else:
        _result = _method(_input)
else:
    if inspect.iscoroutinefunction(_method):
        _result = asyncio.run(_method())
    else:
        _result = _method()

print(json.dumps({
    'result':       _serialize_result(_result),
    'stateChanged': _state_changed,
    'state':        _pending_state if _state_changed else None,
}))
`;
}

function findDeno(): string {
  try {
    const result = execFileSync('which', ['deno'], { encoding: 'utf-8' });
    return result.trim();
  } catch {
    throw new Error(
      'class execution requires Deno. Install from https://deno.land',
    );
  }
}

function findPython(): string {
  for (const name of ['python3', 'python']) {
    try {
      const result = execFileSync('which', [name], { encoding: 'utf-8' });
      return result.trim();
    } catch {
      continue;
    }
  }
  throw new Error('class execution requires Python 3');
}

function buildDenoFlags(classesDir: string, className: string): string[] {
  const denoCache = join(
    process.env.HOME || '/tmp',
    'Library/Caches/deno',
  );

  const flags = [
    `--allow-read=${classesDir},${denoCache}`,
    '--allow-env=OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD',
  ];

  // Check for sidecar permissions file
  const sidecar = join(classesDir, `${className}.json`);
  if (existsSync(sidecar)) {
    try {
      const perms = JSON.parse(readFileSync(sidecar, 'utf-8'));
      if (perms.net === true) flags.push('--allow-net');
      else if (Array.isArray(perms.net))
        flags.push(`--allow-net=${perms.net.join(',')}`);
      if (perms.env === true) flags.push('--allow-env');
      else if (Array.isArray(perms.env))
        flags[1] = `--allow-env=OBJECTIFY_INPUT,OBJECTIFY_STATE,OBJECTIFY_METHOD,${perms.env.join(',')}`;
      if (perms.run === true) flags.push('--allow-run');
      else if (Array.isArray(perms.run))
        flags.push(`--allow-run=${perms.run.join(',')}`);
      if (perms.write === true) flags.push('--allow-write');
      else if (Array.isArray(perms.write))
        flags.push(`--allow-write=${perms.write.join(',')}`);
      if (Array.isArray(perms.read)) {
        flags[0] = `--allow-read=${classesDir},${denoCache},${perms.read.join(',')}`;
      }
    } catch {
      // ignore invalid sidecar
    }
  }

  return flags;
}

export function runClassMethod(opts: {
  classesDir: string;
  className: string;
  method: string;
  state: unknown;
  input?: unknown;
}): ClassResult {
  const { classesDir, className, method, state, input } = opts;
  const classFile = findClassFile(classesDir, className);

  const stateJson = JSON.stringify(state ?? {});
  const inputJson = JSON.stringify(input ?? null);

  const env = {
    ...process.env,
    OBJECTIFY_STATE: stateJson,
    OBJECTIFY_METHOD: method,
    OBJECTIFY_INPUT: inputJson,
  };

  let tmpPath: string;
  let result: string;

  if (classFile.lang === 'ts') {
    const deno = findDeno();
    const script = generateTsRunner(classFile.path);
    tmpPath = tempFile('objectify-runner-', '.ts');
    writeFileSync(tmpPath, script);

    try {
      const flags = buildDenoFlags(classesDir, className);
      result = execFileSync(deno, ['run', ...flags, tmpPath], {
        encoding: 'utf-8',
        env,
      });
    } finally {
      try { unlinkSync(tmpPath); } catch { /* ignore */ }
    }
  } else {
    const python = findPython();
    const script = generatePyRunner(classFile.path, className);
    tmpPath = tempFile('objectify-runner-', '.py');
    writeFileSync(tmpPath, script);

    try {
      result = execFileSync(python, [tmpPath], {
        encoding: 'utf-8',
        env,
      });
    } finally {
      try { unlinkSync(tmpPath); } catch { /* ignore */ }
    }
  }

  return JSON.parse(result.trim()) as ClassResult;
}
