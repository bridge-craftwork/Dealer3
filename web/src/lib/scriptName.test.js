import { describe, it, expect } from 'vitest'
import {
  displayName,
  identityName,
  metadataValue,
  scriptDescription,
  scriptTitle,
} from './scriptName.js'

const PBS = `# alias: NTLadder
# button-text: NT Ladder (Lev)
# scenario-title: --- NT Ladder (Lev)
# gib-works: true

produce 5000
`

describe('scriptTitle', () => {
  it('reads the title statement', () => {
    expect(scriptTitle('title "Weak twos"\ncondition 1\n')).toBe('Weak twos')
  })

  it('finds it after comments and blank lines', () => {
    expect(scriptTitle('# about\n\n  title "Weak twos"\n')).toBe('Weak twos')
  })

  it('does not read one that is commented out', () => {
    expect(scriptTitle('# title "Not me"\ncondition 1\n')).toBe('')
    expect(scriptTitle('/*\ntitle "Not me"\n*/\ncondition 1\n')).toBe('')
  })

  it('is empty when there is none', () => {
    expect(scriptTitle('condition 1\n')).toBe('')
    expect(scriptTitle('')).toBe('')
  })
})

describe('metadataValue', () => {
  it('reads a PBS header, whatever the case of the key', () => {
    expect(metadataValue(PBS, 'alias')).toBe('NTLadder')
    expect(metadataValue('# Button-Text: Hi\n', 'button-text')).toBe('Hi')
  })

  it('is empty when the header is absent', () => {
    expect(metadataValue(PBS, 'convention-card')).toBe('')
  })
})

describe('identityName', () => {
  it('prefers the title statement', () => {
    expect(identityName(`title "Mine"\n${PBS}`)).toBe('Mine')
  })

  it('falls back to the PBS alias, not its display text', () => {
    expect(identityName(PBS)).toBe('NTLadder')
  })

  it('is empty for a script that names itself neither way', () => {
    expect(identityName('# just a comment\ncondition 1\n')).toBe('')
  })
})

describe('displayName', () => {
  it('prefers the title, then button text', () => {
    expect(displayName(`title "Mine"\n${PBS}`)).toBe('Mine')
    expect(displayName(PBS)).toBe('NT Ladder (Lev)')
  })

  it('takes the dashes off a scenario title', () => {
    expect(displayName('# scenario-title: --- Weak Twos\n')).toBe('Weak Twos')
  })
})

describe('scriptDescription', () => {
  it('is the first paragraph of the opening comment, on one line', () => {
    const text = `title "HCP against tricks"
# NS combined high-card points against
# the tricks NS can take.
#
# A second paragraph.

condition 1
`
    expect(scriptDescription(text)).toBe('NS combined high-card points against the tricks NS can take.')
  })

  it('skips PBS headers above the comment', () => {
    expect(scriptDescription(`${PBS}`)).toBe('')
    expect(scriptDescription('# alias: X\n\n# What it does.\ncondition 1\n')).toBe('What it does.')
  })

  it('is empty when the script opens with code', () => {
    expect(scriptDescription('condition 1\n# later\n')).toBe('')
  })
})
