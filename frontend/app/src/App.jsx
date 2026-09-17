/* Phoskonomia — layout route. Renders the permanent CRT skin overlays once
   (the design export put these in <body>) then the active page via <Outlet/>. */
import React from 'react'
import { Outlet } from 'react-router-dom'

export default function App() {
  return (
    <>
      <div className="osc-scan" />
      <div className="osc-roll" />
      <div className="osc-vig" />
      <Outlet />
    </>
  )
}
