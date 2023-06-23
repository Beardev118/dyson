import crypto from 'crypto'
import { tap } from '@/lib/utils'
import Session from '@/lib/session'
import { NextRequest, NextResponse } from 'next/server'

export const GET = async (req: NextRequest): Promise<Response> => {
	const session = await Session.fromRequest(req)
	session.challenge = generateChallenge()

	return tap(NextResponse.json(session.challenge), res => session.persist(res))
}

const generateChallenge = () => {
	return crypto.randomBytes(32).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '')
}
