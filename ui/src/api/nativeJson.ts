import { parse, parseNumberAndBigInt, stringify } from 'lossless-json'

export type NativeInteger = number | bigint

const safeInteger = BigInt(Number.MAX_SAFE_INTEGER)

export function parseNativeJson<T>(text: string): T {
  return parse(text, null, value => {
    const number = parseNumberAndBigInt(value)
    return typeof number === 'bigint' && number >= -safeInteger && number <= safeInteger
      ? Number(number) : number
  }) as T
}

export function stringifyNativeJson(value: unknown, space?: number): string {
  const text = stringify(value, null, space)
  if (text === undefined) throw new TypeError('The native JSON document has no serializable value')
  return text
}

export function nativeTimestamp(milliseconds: NativeInteger | null): string {
  if (milliseconds === null) return '—'
  const value = Number(milliseconds)
  if (!Number.isSafeInteger(value) || Math.abs(value) > 8640000000000000) return String(milliseconds)
  return new Date(value).toLocaleString()
}
