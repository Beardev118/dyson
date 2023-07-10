'use client'

import useSWR from 'swr'
import { APP_NAME } from '@/lib/consts'
import { useRouter } from 'next/navigation'
import { FormEvent, useCallback, useState } from 'react'
import { supported, create } from '@github/webauthn-json'

const Home = () => {
	const router = useRouter()
	const [username, setUsername] = useState('')
	const { data: challenge } = useSWR<string>('/auth')

	const register = useCallback(
		async (event: FormEvent<HTMLFormElement>) => {
			event.preventDefault()

			const available = await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()
			if (!available || !supported() || !challenge) {
				alert("Seems like your browser doesn't support passkeys...")
				throw new Error('WebAuthn is not supported')
			}

			const response = await fetch(`/auth/register/api?username=${encodeURIComponent(username)}`)

			if (!response.ok) {
				const { error } = await response.json()

				alert(error)
				throw new Error(error)
			}

			const credential = await create({
				publicKey: {
					challenge,
					timeout: 60000,
					attestation: 'direct',
					rp: { name: APP_NAME },
					user: { name: username, displayName: username, id: crypto.randomUUID() },
					authenticatorSelection: { residentKey: 'required', userVerification: 'preferred' },
					pubKeyCredParams: [
						{ alg: -7, type: 'public-key' },
						{ alg: -257, type: 'public-key' },
					],
				},
			})

			const result = await fetch('/auth/register/api', {
				method: 'POST',
				body: JSON.stringify({ username, credential }),
				headers: { 'Content-Type': 'application/json' },
			})

			if (result.ok) {
				return router.push('/dashboard')
			}

			alert('Something went wrong...')
		},
		[router, username, challenge]
	)

	return (
		<div className="flex flex-col items-center space-y-8">
			<p className="text-neutral-300">👋 Welcome to {APP_NAME}! Create an account to continue.</p>
			<form onSubmit={register} className="w-full max-w-md">
				<label htmlFor="username" className="block text-neutral-300">
					Username
				</label>
				<input
					required
					autoFocus
					type="text"
					id="username"
					value={username}
					placeholder="@miguel"
					pattern="^@[a-zA-Z][a-zA-Z0-9._]{2,14}$"
					onChange={event => setUsername(event.target.value)}
					className="mt-1 block w-full p-3 bg-neutral-800 text-neutral-300 placeholder:text-neutral-600 focus:outline-none"
				/>
				<p className="mt-1 text-sm text-neutral-600">
					Starts with @ and can only contain letters, numbers, dots and underscores.
				</p>
				<div className="mt-2 flex w-full justify-end">
					<button type="submit" className="bg-neutral-800  text-neutral-400 px-5 py-2 text-lg">
						Register
					</button>
				</div>
			</form>
		</div>
	)
}

export default Home
