import { describe, expect, it } from 'vitest'
import { nativeTimestamp, parseNativeJson, stringifyNativeJson } from './nativeJson'

describe('native JSON integer precision', () => {
  it('keeps float scores, bounded integers and arithmetic numeric while preserving unsafe integer identity', () => {
    const parsed = parseNativeJson<{ score: number; confidence: number; units: number; exponent: number; generation: bigint }>('{"score":0.625,"confidence":1.25e-3,"units":4294967295,"exponent":1e20,"generation":9007199254740993}')
    expect(parsed.score * 100).toBe(62.5)
    expect(parsed.confidence + 0.00125).toBe(0.0025)
    expect(parsed.units + 1).toBe(4294967296)
    expect(typeof parsed.exponent).toBe('number')
    expect(parsed.generation).toBe(9007199254740993n)
  })
  it('keeps small integer arrays numeric and serializes large signed and unsigned integers exactly', () => {
    const text = '{"units":[47,255,65535],"revision":18446744073709551615,"negative":-9223372036854775808,"safe":9007199254740991,"next":9007199254740993,"false":false,"text":"18446744073709551615","empty":null}'
    const parsed = parseNativeJson<{ units: number[]; revision: bigint; negative: bigint; safe: number; next: bigint }>(text)
    expect(parsed.units).toEqual([47, 255, 65535])
    expect(parsed.revision).toBe(18446744073709551615n)
    expect(parsed.negative).toBe(-9223372036854775808n)
    expect(parsed.safe).toBe(9007199254740991)
    expect(parsed.next).toBe(9007199254740993n)
    expect(stringifyNativeJson(parsed)).toBe(text)
  })

  it('rejects malformed JSON and duplicate keys instead of constructing a partial document', () => {
    expect(() => parseNativeJson('{"revision":18446744073709551615,')).toThrow()
    expect(() => parseNativeJson('{"revision":1,"revision":2}')).toThrow()
    expect(() => stringifyNativeJson(undefined)).toThrow('no serializable value')
  })

  it('formats actual millisecond dates and retains timestamps outside the Date range literally', () => {
    expect(nativeTimestamp(0)).toBe(new Date(0).toLocaleString())
    expect(nativeTimestamp(null)).toBe('—')
    expect(nativeTimestamp(18446744073709551615n)).toBe('18446744073709551615')
    expect(nativeTimestamp(8640000000000001)).toBe('8640000000000001')
  })
})
